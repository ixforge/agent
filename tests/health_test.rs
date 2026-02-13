use serde_json::Value;

#[tokio::test]
async fn test_health_endpoint_returns_json() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        ixforge_agent::metrics::server::run_with_listener(listener).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let resp = reqwest::get(format!("http://{addr}/health"))
        .await
        .unwrap();
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
