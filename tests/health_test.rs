use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::Value;

#[tokio::test]
async fn test_health_endpoint_returns_json() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        ixforge_agent::metrics::server::run_with_listener(listener).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let resp = reqwest::get(format!("http://{addr}/health")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body.get("version").is_some());
    assert!(body.get("uptime_seconds").is_some());
    assert!(body.get("bird").is_some());
    assert!(body.get("core_connected").is_some());

    server_handle.abort();
}

#[tokio::test]
async fn test_metrics_endpoint_returns_prometheus_format() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        ixforge_agent::metrics::server::run_with_listener(listener).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let resp = reqwest::get(format!("http://{addr}/metrics"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("ixforge_agent_uptime_seconds"),
        "metrics should contain agent uptime: {body}"
    );

    server_handle.abort();
}

#[tokio::test]
async fn test_metrics_include_per_peer_bgp_and_config_info() {
    use ixforge_agent::bird::parser::{BgpState, BirdProtocol};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let metrics = ixforge_agent::metrics::registry::MetricsRegistry::new();

    let protocols = vec![
        BirdProtocol {
            name: "peer_as64500".to_string(),
            proto: "BGP".to_string(),
            state: BgpState::Up,
            neighbor_address: Some("10.0.0.1".to_string()),
            neighbor_asn: Some(64500),
            prefixes_imported: Some(42),
            prefixes_exported: Some(15),
        },
        BirdProtocol {
            name: "peer_as64501".to_string(),
            proto: "BGP".to_string(),
            state: BgpState::Down,
            neighbor_address: Some("10.0.0.2".to_string()),
            neighbor_asn: Some(64501),
            prefixes_imported: Some(0),
            prefixes_exported: Some(0),
        },
    ];
    metrics.update_bgp_peers(&protocols);
    metrics.set_config_applied("abc123def456");

    let core_connected = Arc::new(AtomicBool::new(true));
    let bird_running = Arc::new(AtomicBool::new(true));

    let server_handle = tokio::spawn({
        let metrics = Arc::clone(&metrics);
        let core_connected = Arc::clone(&core_connected);
        let bird_running = Arc::clone(&bird_running);
        async move {
            ixforge_agent::metrics::server::run_with_state(
                listener,
                metrics,
                core_connected,
                bird_running,
            )
            .await;
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let resp = reqwest::get(format!("http://{addr}/metrics"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.text().await.unwrap();

    // Per-peer BGP session state
    assert!(
        body.contains(r#"ixforge_agent_bgp_session_state{peer="10.0.0.1",asn="64500"} 1"#),
        "missing bgp_session_state for up peer: {body}"
    );
    assert!(
        body.contains(r#"ixforge_agent_bgp_session_state{peer="10.0.0.2",asn="64501"} 0"#),
        "missing bgp_session_state for down peer: {body}"
    );

    // Per-peer prefixes
    assert!(
        body.contains(r#"ixforge_agent_bgp_prefixes_imported{peer="10.0.0.1",asn="64500"} 42"#),
        "missing prefixes_imported: {body}"
    );
    assert!(
        body.contains(r#"ixforge_agent_bgp_prefixes_exported{peer="10.0.0.1",asn="64500"} 15"#),
        "missing prefixes_exported: {body}"
    );

    // Config info
    assert!(
        body.contains(r#"ixforge_agent_config_info{config_hash="abc123def456"} 1"#),
        "missing config_info: {body}"
    );

    // Config last applied timestamp (should be non-zero)
    assert!(
        body.contains("ixforge_agent_config_last_applied_timestamp"),
        "missing config_last_applied_timestamp: {body}"
    );

    // Aggregate BGP gauges still present
    assert!(
        body.contains("ixforge_agent_bgp_sessions_up"),
        "missing aggregate bgp_sessions_up: {body}"
    );
    assert!(
        body.contains("ixforge_agent_bgp_sessions_total"),
        "missing aggregate bgp_sessions_total: {body}"
    );

    server_handle.abort();
}

#[tokio::test]
async fn test_metrics_no_bgp_peers_omits_per_peer_section() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let metrics = ixforge_agent::metrics::registry::MetricsRegistry::new();
    let core_connected = Arc::new(AtomicBool::new(false));
    let bird_running = Arc::new(AtomicBool::new(false));

    let server_handle = tokio::spawn({
        let metrics = Arc::clone(&metrics);
        let core_connected = Arc::clone(&core_connected);
        let bird_running = Arc::clone(&bird_running);
        async move {
            ixforge_agent::metrics::server::run_with_state(
                listener,
                metrics,
                core_connected,
                bird_running,
            )
            .await;
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let resp = reqwest::get(format!("http://{addr}/metrics"))
        .await
        .unwrap();
    let body = resp.text().await.unwrap();

    // No per-peer metrics when no peers registered
    assert!(
        !body.contains("ixforge_agent_bgp_session_state{"),
        "should not have per-peer metrics without peers: {body}"
    );

    // No config info when no config applied
    assert!(
        !body.contains("ixforge_agent_config_info{"),
        "should not have config_info without applied config: {body}"
    );

    // Aggregate gauges still present (at zero)
    assert!(body.contains("ixforge_agent_bgp_sessions_up"));
    assert!(body.contains("ixforge_agent_bgp_sessions_total"));

    server_handle.abort();
}
