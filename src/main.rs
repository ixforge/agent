use std::path::Path;
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;
use tokio::net::TcpListener;
use tokio::time::{Duration, sleep};
use tracing::{error, info, warn};

use ixforge_agent::bird::manager::BirdManager;
use ixforge_agent::bird::socket::BirdSocketClient;
use ixforge_agent::config::AgentConfig;
use ixforge_agent::core_client::{
    BgpSessionState, BirdInstanceStatus, ConfigApplied, CoreClient, Heartbeat, StatusReport,
};
use ixforge_agent::metrics::registry::MetricsRegistry;
use ixforge_agent::state::AgentState;

#[derive(Parser)]
#[command(name = "ixforge-agent", version, about = "IXForge route server agent")]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "/etc/ixforge-agent/config.toml")]
    config: String,
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "failed to install SIGTERM handler");
                process::exit(1);
            }
        };
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                if let Err(e) = result {
                    error!(error = %e, "failed to listen for SIGINT");
                    return;
                }
                info!("received SIGINT, shutting down");
            }
            _ = sigterm.recv() => {
                info!("received SIGTERM, shutting down");
            }
        }
    }

    #[cfg(not(unix))]
    {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!(error = %e, "failed to listen for ctrl-c");
            return;
        }
        info!("received ctrl-c, shutting down");
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let config = match AgentConfig::from_file(Path::new(&cli.config)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config from {}: {e}", cli.config);
            process::exit(1);
        }
    };

    let _log_guards = ixforge_agent::logging::init(&config.logging);

    info!(
        version = env!("CARGO_PKG_VERSION"),
        config_path = %cli.config,
        core_url = %config.core.url,
        route_server_id = %config.core.route_server_id,
        poll_interval = config.core.poll_interval_secs,
        "ixforge-agent starting"
    );

    let core_client = match CoreClient::new(
        &config.core.url,
        &config.core.api_key,
        &config.core.route_server_id,
        config.core.ca_cert_path.as_deref(),
    ) {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "failed to initialize Core API client");
            process::exit(1);
        }
    };

    let bird_socket =
        BirdSocketClient::new(&config.bird.socket_path, config.bird.socket_timeout_secs);
    let bird_manager = BirdManager::new(
        bird_socket,
        &config.bird.config_path,
        &config.bird.bird_binary,
    );

    let mut state = AgentState::new();

    // Shared state for metrics server
    let metrics = MetricsRegistry::new();
    let core_connected = Arc::new(AtomicBool::new(false));
    let bird_running_flag = Arc::new(AtomicBool::new(false));

    // Start metrics server in background
    let metrics_listen = config.metrics.listen.clone();
    let metrics_clone = Arc::clone(&metrics);
    let core_connected_clone = Arc::clone(&core_connected);
    let bird_running_clone = Arc::clone(&bird_running_flag);

    tokio::spawn(async move {
        let listener = match TcpListener::bind(&metrics_listen).await {
            Ok(l) => l,
            Err(e) => {
                error!(error = %e, listen = %metrics_listen, "failed to bind metrics server");
                return;
            }
        };
        ixforge_agent::metrics::server::run_with_state(
            listener,
            metrics_clone,
            core_connected_clone,
            bird_running_clone,
        )
        .await;
    });

    let interval = Duration::from_secs(config.core.poll_interval_secs);
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    info!("entering main polling loop");

    loop {
        // --- Step 1: Config polling ---
        match core_client.get_config().await {
            Ok(config_resp) => {
                core_connected.store(true, Ordering::Relaxed);

                if config_resp.config_hash != state.current_config_hash {
                    info!(
                        new_hash = %config_resp.config_hash,
                        old_hash = %state.current_config_hash,
                        "new config detected"
                    );

                    let temp_path = format!("{}.tmp", config.bird.config_path);
                    if let Err(e) = tokio::fs::write(&temp_path, &config_resp.content).await {
                        error!(error = %e, path = %temp_path, "failed to write temp config file");
                        metrics.poll_errors_total.inc();
                    } else {
                        match bird_manager.validate_config(Path::new(&temp_path)).await {
                            Ok(()) => {
                                info!("config validation passed");

                                if let Err(e) =
                                    bird_manager.write_config(&config_resp.content).await
                                {
                                    error!(error = %e, "failed to write config");
                                    metrics.poll_errors_total.inc();
                                } else {
                                    match bird_manager.apply_config().await {
                                        Ok(()) => {
                                            state.current_config_hash =
                                                config_resp.config_hash.clone();
                                            metrics.set_config_applied(&config_resp.config_hash);

                                            let applied = ConfigApplied {
                                                config_hash: config_resp.config_hash,
                                            };
                                            if let Err(e) =
                                                core_client.confirm_config_applied(&applied).await
                                            {
                                                warn!(error = %e, "failed to confirm config applied");
                                            }
                                        }
                                        Err(e) => {
                                            error!(error = %e, "failed to apply config via birdc");
                                            metrics.poll_errors_total.inc();
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!(error = %e, "config validation failed, keeping previous config");
                                metrics.poll_errors_total.inc();
                            }
                        }

                        let _ = tokio::fs::remove_file(&temp_path).await;
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "failed to poll config from Core");
                core_connected.store(false, Ordering::Relaxed);
                metrics.poll_errors_total.inc();
            }
        }

        // --- Step 2: BGP status reporting ---
        let bird_is_running = bird_manager.is_running().await;
        bird_running_flag.store(bird_is_running, Ordering::Relaxed);
        let bird_uptime = bird_manager.get_uptime().await;

        match bird_manager.get_protocols().await {
            Ok(protocols) => {
                let sessions: Vec<BgpSessionState> = protocols
                    .iter()
                    .filter_map(|p| {
                        p.neighbor_address.as_ref().map(|addr| BgpSessionState {
                            peer_ip: addr.clone(),
                            oper_state: p.state.as_oper_state().to_string(),
                            af: if addr.contains(':') { 6 } else { 4 },
                        })
                    })
                    .collect();

                if !sessions.is_empty() {
                    let report = StatusReport { sessions };
                    if let Err(e) = core_client.report_status(&report).await {
                        warn!(error = %e, "failed to report BGP status");
                    }
                }

                metrics.update_bgp_peers(&protocols);
                state.last_protocols = protocols;
            }
            Err(e) => {
                warn!(error = %e, "failed to get BIRD protocols");
            }
        }

        // --- Step 3: Heartbeat ---
        let heartbeat = Heartbeat {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: state.uptime_seconds(),
            current_config_hash: if state.has_config() {
                state.current_config_hash.clone()
            } else {
                "0".repeat(64)
            },
            bird_instances: vec![BirdInstanceStatus {
                name: "bird".to_string(),
                running: bird_is_running,
                uptime_seconds: bird_uptime,
            }],
        };

        match core_client.send_heartbeat_with_headers(&heartbeat).await {
            Ok((response, upgrade_version)) => {
                if !response.config_hash_match && state.has_config() {
                    warn!("config hash mismatch reported by Core");
                }
                if let Some(version) = upgrade_version {
                    warn!(required_version = %version, "Core requests agent upgrade");
                }
            }
            Err(e) => {
                warn!(error = %e, "failed to send heartbeat");
            }
        }

        // Wait for next poll cycle or shutdown signal
        tokio::select! {
            _ = sleep(interval) => {}
            _ = &mut shutdown => {
                break;
            }
        }
    }

    info!("ixforge-agent stopped");
}
