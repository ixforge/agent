use std::fmt::Write;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;

use crate::bird::parser::{BgpState, BirdProtocol};
use super::system::SystemMetrics;

/// Per-peer BGP snapshot for metrics rendering
#[derive(Debug, Clone)]
struct BgpPeerSnapshot {
    peer: String,
    asn: String,
    state: i64,
    prefixes_imported: i64,
    prefixes_exported: i64,
}

pub struct MetricsRegistry {
    registry: Registry,
    pub agent_uptime: Gauge,
    pub poll_errors_total: Counter,
    pub bgp_sessions_up: Gauge,
    pub bgp_sessions_total: Gauge,
    pub host_cpu_usage: Gauge<f64, AtomicU64>,
    pub host_memory_usage: Gauge<f64, AtomicU64>,
    pub config_last_applied: Gauge<f64, AtomicU64>,
    bgp_peers: Mutex<Vec<BgpPeerSnapshot>>,
    config_hash: Mutex<String>,
    started_at: Instant,
    system_metrics: Mutex<SystemMetrics>,
}

impl MetricsRegistry {
    pub fn new() -> Arc<Self> {
        let mut registry = Registry::default();

        let agent_uptime = Gauge::default();
        registry.register(
            "ixforge_agent_uptime_seconds",
            "Agent uptime in seconds",
            agent_uptime.clone(),
        );

        let poll_errors_total = Counter::default();
        registry.register(
            "ixforge_agent_poll_errors_total",
            "Total number of Core API poll errors",
            poll_errors_total.clone(),
        );

        let bgp_sessions_up = Gauge::default();
        registry.register(
            "ixforge_agent_bgp_sessions_up",
            "Number of BGP sessions in established state",
            bgp_sessions_up.clone(),
        );

        let bgp_sessions_total = Gauge::default();
        registry.register(
            "ixforge_agent_bgp_sessions_total",
            "Total number of BGP sessions",
            bgp_sessions_total.clone(),
        );

        let host_cpu_usage = Gauge::<f64, _>::default();
        registry.register(
            "ixforge_agent_host_cpu_usage_percent",
            "Host CPU usage percentage",
            host_cpu_usage.clone(),
        );

        let host_memory_usage = Gauge::<f64, _>::default();
        registry.register(
            "ixforge_agent_host_memory_usage_percent",
            "Host memory usage percentage",
            host_memory_usage.clone(),
        );

        let config_last_applied = Gauge::<f64, _>::default();
        registry.register(
            "ixforge_agent_config_last_applied_timestamp",
            "Unix timestamp of last successful config application",
            config_last_applied.clone(),
        );

        Arc::new(Self {
            registry,
            agent_uptime,
            poll_errors_total,
            bgp_sessions_up,
            bgp_sessions_total,
            host_cpu_usage,
            host_memory_usage,
            config_last_applied,
            bgp_peers: Mutex::new(Vec::new()),
            config_hash: Mutex::new(String::new()),
            started_at: Instant::now(),
            system_metrics: Mutex::new(SystemMetrics::new()),
        })
    }

    /// Update per-peer BGP metrics and aggregate gauges from parsed BIRD protocols
    pub fn update_bgp_peers(&self, protocols: &[BirdProtocol]) {
        let peers: Vec<BgpPeerSnapshot> = protocols
            .iter()
            .filter_map(|p| {
                let peer = p.neighbor_address.as_ref()?;
                Some(BgpPeerSnapshot {
                    peer: peer.clone(),
                    asn: p.neighbor_asn.map_or_else(String::new, |a| a.to_string()),
                    state: if p.state == BgpState::Up { 1 } else { 0 },
                    prefixes_imported: p.prefixes_imported.unwrap_or(0) as i64,
                    prefixes_exported: p.prefixes_exported.unwrap_or(0) as i64,
                })
            })
            .collect();

        let up_count = peers.iter().filter(|p| p.state == 1).count();
        self.bgp_sessions_up.set(up_count as i64);
        self.bgp_sessions_total.set(peers.len() as i64);

        *self.bgp_peers.lock().unwrap() = peers;
    }

    /// Record a successful config application
    pub fn set_config_applied(&self, hash: &str) {
        *self.config_hash.lock().unwrap() = hash.to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        self.config_last_applied.set(now);
    }

    /// Update dynamic metrics before encoding
    pub fn refresh(&self) {
        self.agent_uptime
            .set(self.started_at.elapsed().as_secs() as i64);

        if let Ok(mut sys) = self.system_metrics.lock() {
            sys.refresh();
            self.host_cpu_usage.set(sys.cpu_usage());
            self.host_memory_usage.set(sys.memory_usage_percent());
        }
    }

    pub fn encode(&self) -> String {
        self.refresh();
        let mut buf = String::new();
        encode(&mut buf, &self.registry).unwrap_or_default();
        self.encode_bgp_peers(&mut buf);
        self.encode_config_info(&mut buf);
        buf
    }

    fn encode_bgp_peers(&self, buf: &mut String) {
        let peers = self.bgp_peers.lock().unwrap();
        if peers.is_empty() {
            return;
        }

        let _ = writeln!(
            buf,
            "# HELP ixforge_agent_bgp_session_state BGP session state (1=up, 0=down)"
        );
        let _ = writeln!(buf, "# TYPE ixforge_agent_bgp_session_state gauge");
        for p in peers.iter() {
            let _ = writeln!(
                buf,
                "ixforge_agent_bgp_session_state{{peer=\"{}\",asn=\"{}\"}} {}",
                escape_label_value(&p.peer),
                escape_label_value(&p.asn),
                p.state
            );
        }

        let _ = writeln!(
            buf,
            "# HELP ixforge_agent_bgp_prefixes_imported Imported prefixes per BGP peer"
        );
        let _ = writeln!(buf, "# TYPE ixforge_agent_bgp_prefixes_imported gauge");
        for p in peers.iter() {
            let _ = writeln!(
                buf,
                "ixforge_agent_bgp_prefixes_imported{{peer=\"{}\",asn=\"{}\"}} {}",
                escape_label_value(&p.peer),
                escape_label_value(&p.asn),
                p.prefixes_imported
            );
        }

        let _ = writeln!(
            buf,
            "# HELP ixforge_agent_bgp_prefixes_exported Exported prefixes per BGP peer"
        );
        let _ = writeln!(buf, "# TYPE ixforge_agent_bgp_prefixes_exported gauge");
        for p in peers.iter() {
            let _ = writeln!(
                buf,
                "ixforge_agent_bgp_prefixes_exported{{peer=\"{}\",asn=\"{}\"}} {}",
                escape_label_value(&p.peer),
                escape_label_value(&p.asn),
                p.prefixes_exported
            );
        }
    }

    fn encode_config_info(&self, buf: &mut String) {
        let hash = self.config_hash.lock().unwrap();
        if hash.is_empty() {
            return;
        }

        let _ = writeln!(
            buf,
            "# HELP ixforge_agent_config_info Current applied configuration info"
        );
        let _ = writeln!(buf, "# TYPE ixforge_agent_config_info gauge");
        let _ = writeln!(
            buf,
            "ixforge_agent_config_info{{config_hash=\"{}\"}} 1",
            escape_label_value(&hash)
        );
    }
}

/// Escape a Prometheus label value per the exposition format spec
fn escape_label_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
