use ixforge_agent::core_client::{
    BgpSessionState, BirdInstanceStatus, ConfigApplied, CoreClient, Heartbeat, StatusReport,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_heartbeat() -> Heartbeat {
    Heartbeat {
        version: "0.1.0".to_string(),
        uptime_seconds: 3600.0,
        current_config_hash: "a".repeat(64),
        bird_instances: vec![BirdInstanceStatus {
            name: "bird".to_string(),
            running: true,
            uptime_seconds: Some(3600.0),
        }],
    }
}

const RS_ID: &str = "550e8400-e29b-41d4-a716-446655440000";

fn make_client(base_url: &str) -> CoreClient {
    CoreClient::new(base_url, "test-key", &RS_ID.parse().unwrap(), None).unwrap()
}

#[tokio::test]
async fn test_get_config_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/route-servers/{RS_ID}/agent/config")))
        .and(header("X-API-Key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "config_hash": "abc123",
            "content": "router id 10.0.0.1;",
            "generated_at": "2024-01-15T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let config = client.get_config().await.unwrap();
    assert_eq!(config.config_hash, "abc123");
    assert_eq!(config.content, "router id 10.0.0.1;");
    assert!(client.last_poll_ok());
}

#[tokio::test]
async fn test_get_config_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/route-servers/{RS_ID}/agent/config")))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": { "code": "NOT_FOUND", "message": "No config exists" }
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let result = client.get_config().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_report_status_success() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/route-servers/{RS_ID}/agent/status")))
        .and(header("X-API-Key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "updated": 1, "unchanged": 1, "not_found": 0
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let report = StatusReport {
        sessions: vec![
            BgpSessionState {
                peer_ip: "10.0.0.1".into(),
                oper_state: "up".into(),
            },
            BgpSessionState {
                peer_ip: "10.0.0.2".into(),
                oper_state: "down".into(),
            },
        ],
    };
    let response = client.report_status(&report).await.unwrap();
    assert_eq!(response.updated, 1);
    assert_eq!(response.unchanged, 1);
}

#[tokio::test]
async fn test_heartbeat_success() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/api/v1/route-servers/{RS_ID}/agent/heartbeat"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "acknowledged": true, "config_hash_match": true
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let response = client.send_heartbeat(&test_heartbeat()).await.unwrap();
    assert!(response.acknowledged);
    assert!(response.config_hash_match);
}

#[tokio::test]
async fn test_heartbeat_upgrade_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/api/v1/route-servers/{RS_ID}/agent/heartbeat"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-IXForge-Agent-Upgrade", "0.2.0")
                .set_body_json(serde_json::json!({
                    "acknowledged": true, "config_hash_match": false
                })),
        )
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let (response, upgrade_version) = client
        .send_heartbeat_with_headers(&test_heartbeat())
        .await
        .unwrap();
    assert!(response.acknowledged);
    assert!(!response.config_hash_match);
    assert_eq!(upgrade_version.as_deref(), Some("0.2.0"));
}

#[tokio::test]
async fn test_confirm_config_applied() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/api/v1/route-servers/{RS_ID}/agent/config/applied"
        )))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let body = ConfigApplied {
        config_hash: "a".repeat(64),
    };
    client.confirm_config_applied(&body).await.unwrap();
}

#[tokio::test]
async fn test_core_unreachable() {
    let client = make_client("http://127.0.0.1:1");
    let result = client.get_config().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_last_poll_ok_resets_on_failure() {
    let server = MockServer::start().await;
    let config_path = format!("/api/v1/route-servers/{RS_ID}/agent/config");

    // Step 1: successful poll
    let guard = Mock::given(method("GET"))
        .and(path(&config_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "config_hash": "abc123",
            "content": "router id 10.0.0.1;",
            "generated_at": "2024-01-15T00:00:00Z"
        })))
        .mount_as_scoped(&server)
        .await;

    let client = make_client(&server.uri());
    let _ = client.get_config().await.unwrap();
    assert!(client.last_poll_ok(), "should be true after success");

    // Step 2: drop success mock, mount 500 error
    drop(guard);
    Mock::given(method("GET"))
        .and(path(&config_path))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let _ = client.get_config().await;
    assert!(!client.last_poll_ok(), "should be false after failure");
}

#[tokio::test]
async fn test_auth_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/route-servers/{RS_ID}/agent/config")))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": { "code": "UNAUTHORIZED", "message": "Invalid API key" }
        })))
        .mount(&server)
        .await;

    let client =
        CoreClient::new(&server.uri(), "wrong-key", &RS_ID.parse().unwrap(), None).unwrap();
    let result = client.get_config().await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("401") || err_msg.contains("UNAUTHORIZED"),
        "Error should mention auth failure: {err_msg}"
    );
}
