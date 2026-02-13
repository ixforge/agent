# IXForge Agent v0.1 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Rust agent that polls BIRD config from the Core, validates and applies it, reports BGP session state, sends heartbeats, and exposes Prometheus metrics + health endpoint.

**Architecture:** Single-binary daemon. Tokio async runtime with a main polling loop. Traits abstract BIRD interaction for testability. reqwest for Core API, axum for metrics/health HTTP server. One BIRD instance per agent (configurable path). Logging via tracing to stdout + optional file.

**Tech Stack:** Rust (MSRV 1.85+), tokio, reqwest, axum, serde, tracing, prometheus-client, sysinfo, wiremock (tests)

**Decisions from owner:**
- BIRD 2.x only
- One BIRD instance per agent (one socket, one config file)
- TLS: public CA default + custom CA bundle via config
- Config path: fully configurable in config.toml
- Validation failure: log + keep previous config, no event to Core
- Testing: traits for BIRD abstraction + integration with BIRD in Docker
- BIRD comms: Unix socket for birdc, subprocess for bird -p
- Parse: BGP state + prefixes imported/exported
- Retry: fixed interval (no backoff)
- Metrics: BIRD/BGP + host (cpu, memory)
- Health: version + uptime + BIRD status + Core connectivity
- Registration: manual UUID in config.toml
- Config reload: restart process (no SIGHUP)
- No dry-run for v0.1
- amd64 only for now

---

## Task 1: Project Scaffold + Cargo.toml

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: `src/config.rs` (empty)
- Create: `src/error.rs` (empty)
- Create: `src/core_client.rs` (empty)
- Create: `src/state.rs` (empty)
- Create: `src/bird/mod.rs` (empty)
- Create: `src/bird/parser.rs` (empty)
- Create: `src/bird/socket.rs` (empty)
- Create: `src/bird/manager.rs` (empty)
- Create: `src/metrics/mod.rs` (empty)
- Create: `src/metrics/server.rs` (empty)
- Create: `src/metrics/registry.rs` (empty)
- Create: `src/metrics/system.rs` (empty)
- Create: `config.toml.example`
- Create: `.gitignore`

**Step 1: Create Cargo.toml**

```toml
[package]
name = "ixforge-agent"
version = "0.1.0"
edition = "2024"
description = "IXForge route server agent - manages BIRD configurations and reports BGP state"
license = "Apache-2.0"
repository = "https://github.com/ixforge/agent"

[[bin]]
name = "ixforge-agent"
path = "src/main.rs"

[dependencies]
axum = "0.8"
clap = { version = "4", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
prometheus-client = "0.23"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
sysinfo = "0.34"
thiserror = "2"
tokio = { version = "1", features = ["full"] }
toml = "0.8"
tracing = "0.1"
tracing-appender = "0.2"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
uuid = { version = "1", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
wiremock = "0.6"
tokio-test = "0.4"
```

**Step 2: Create .gitignore**

```
/target
```

**Step 3: Create config.toml.example**

```toml
# /etc/ixforge-agent/config.toml

[core]
# URL of the IXForge Core API
url = "https://portal.tuixp.net"
# API key with agent:route_server scope
api_key = "ixf_ag_xxxxxxxxxxxx"
# UUID of the route server this agent manages
route_server_id = "550e8400-e29b-41d4-a716-446655440000"
# Polling interval in seconds
poll_interval_secs = 30
# Optional: path to CA certificate bundle for custom/internal CAs
# ca_cert_path = "/etc/ixforge-agent/ca.pem"

[bird]
# Path to the BIRD 2 control socket
socket_path = "/run/bird/bird.ctl"
# Path to the BIRD configuration file (agent writes here)
config_path = "/etc/bird/bird.conf"
# Path to the bird binary (for config validation with bird -p)
bird_binary = "/usr/sbin/bird"

[metrics]
# Listen address for Prometheus metrics and health endpoint
listen = "0.0.0.0:9100"

[logging]
# Log level: trace, debug, info, warn, error
level = "info"
# Log format: json or text
format = "json"
# Optional: log to file in addition to stdout
# file_path = "/var/log/ixforge-agent.log"
```

**Step 4: Create src/main.rs with minimal entry point**

```rust
use clap::Parser;

#[derive(Parser)]
#[command(name = "ixforge-agent", version, about = "IXForge route server agent")]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "/etc/ixforge-agent/config.toml")]
    config: String,
}

fn main() {
    let _cli = Cli::parse();
    println!("ixforge-agent starting...");
}
```

**Step 5: Create src/lib.rs with module declarations**

```rust
pub mod bird;
pub mod config;
pub mod core_client;
pub mod error;
pub mod metrics;
pub mod state;
```

**Step 6: Create empty module files**

Each module file (`config.rs`, `error.rs`, `core_client.rs`, `state.rs`, `bird/mod.rs`, `bird/parser.rs`, `bird/socket.rs`, `bird/manager.rs`, `metrics/mod.rs`, `metrics/server.rs`, `metrics/registry.rs`, `metrics/system.rs`) starts empty or with minimal module exports:

`src/bird/mod.rs`:
```rust
pub mod manager;
pub mod parser;
pub mod socket;
```

`src/metrics/mod.rs`:
```rust
pub mod registry;
pub mod server;
pub mod system;
```

All other files: empty.

**Step 7: Verify it compiles**

Run: `cargo build`
Expected: compiles successfully

**Step 8: Commit**

```bash
git add -A
git commit -m "feat: project scaffold with Cargo.toml and module structure"
```

---

## Task 2: Error Types

**Files:**
- Modify: `src/error.rs`

**Step 1: Write error types**

```rust
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("config error: {0}")]
    Config(String),

    #[error("core API error: {0}")]
    CoreApi(String),

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("BIRD socket error: {0}")]
    BirdSocket(String),

    #[error("BIRD validation failed: {0}")]
    BirdValidation(String),

    #[error("BIRD command failed: {0}")]
    BirdCommand(String),

    #[error("IO error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("parse error: {0}")]
    Parse(String),
}

impl AgentError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles

**Step 3: Commit**

```bash
git add src/error.rs
git commit -m "feat: define AgentError types"
```

---

## Task 3: Config Parsing with Tests

**Files:**
- Modify: `src/config.rs`
- Create: `tests/config_test.rs`

**Step 1: Write the failing tests**

`tests/config_test.rs`:
```rust
use ixforge_agent::config::AgentConfig;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

#[test]
fn test_parse_minimal_config() {
    let f = write_config(
        r#"
[core]
url = "https://portal.example.net"
api_key = "ixf_ag_test123"
route_server_id = "550e8400-e29b-41d4-a716-446655440000"

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"

[metrics]
listen = "0.0.0.0:9100"

[logging]
level = "info"
format = "json"
"#,
    );

    let config = AgentConfig::from_file(f.path()).unwrap();
    assert_eq!(config.core.url, "https://portal.example.net");
    assert_eq!(config.core.poll_interval_secs, 30); // default
    assert_eq!(config.core.ca_cert_path, None);
    assert_eq!(config.bird.bird_binary, "/usr/sbin/bird"); // default
    assert_eq!(config.logging.file_path, None);
}

#[test]
fn test_parse_config_with_all_options() {
    let f = write_config(
        r#"
[core]
url = "https://portal.example.net"
api_key = "ixf_ag_test123"
route_server_id = "550e8400-e29b-41d4-a716-446655440000"
poll_interval_secs = 60
ca_cert_path = "/etc/ixforge-agent/ca.pem"

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"
bird_binary = "/usr/local/sbin/bird"

[metrics]
listen = "127.0.0.1:9100"

[logging]
level = "debug"
format = "text"
file_path = "/var/log/ixforge-agent.log"
"#,
    );

    let config = AgentConfig::from_file(f.path()).unwrap();
    assert_eq!(config.core.poll_interval_secs, 60);
    assert_eq!(
        config.core.ca_cert_path.as_deref(),
        Some("/etc/ixforge-agent/ca.pem")
    );
    assert_eq!(config.bird.bird_binary, "/usr/local/sbin/bird");
    assert_eq!(config.logging.level, "debug");
    assert_eq!(
        config.logging.file_path.as_deref(),
        Some("/var/log/ixforge-agent.log")
    );
}

#[test]
fn test_parse_config_missing_required_field() {
    let f = write_config(
        r#"
[core]
url = "https://portal.example.net"
# missing api_key and route_server_id

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"

[metrics]
listen = "0.0.0.0:9100"

[logging]
level = "info"
format = "json"
"#,
    );

    let result = AgentConfig::from_file(f.path());
    assert!(result.is_err());
}

#[test]
fn test_parse_config_invalid_uuid() {
    let f = write_config(
        r#"
[core]
url = "https://portal.example.net"
api_key = "ixf_ag_test123"
route_server_id = "not-a-uuid"

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"

[metrics]
listen = "0.0.0.0:9100"

[logging]
level = "info"
format = "json"
"#,
    );

    let result = AgentConfig::from_file(f.path());
    assert!(result.is_err());
}

#[test]
fn test_parse_config_file_not_found() {
    let result = AgentConfig::from_file("/nonexistent/config.toml".as_ref());
    assert!(result.is_err());
}

#[test]
fn test_poll_interval_must_be_positive() {
    let f = write_config(
        r#"
[core]
url = "https://portal.example.net"
api_key = "ixf_ag_test123"
route_server_id = "550e8400-e29b-41d4-a716-446655440000"
poll_interval_secs = 0

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"

[metrics]
listen = "0.0.0.0:9100"

[logging]
level = "info"
format = "json"
"#,
    );

    let result = AgentConfig::from_file(f.path());
    assert!(result.is_err());
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test config_test`
Expected: FAIL (config module is empty)

**Step 3: Implement config parsing**

`src/config.rs`:
```rust
use std::path::Path;

use serde::Deserialize;
use uuid::Uuid;

use crate::error::AgentError;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub core: CoreConfig,
    pub bird: BirdConfig,
    pub metrics: MetricsConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoreConfig {
    pub url: String,
    pub api_key: String,
    pub route_server_id: Uuid,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    pub ca_cert_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BirdConfig {
    pub socket_path: String,
    pub config_path: String,
    #[serde(default = "default_bird_binary")]
    pub bird_binary: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    pub listen: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
    pub file_path: Option<String>,
}

fn default_poll_interval() -> u64 {
    30
}

fn default_bird_binary() -> String {
    "/usr/sbin/bird".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "json".to_string()
}

impl AgentConfig {
    pub fn from_file(path: &Path) -> Result<Self, AgentError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| AgentError::io(path, e))?;
        let config: AgentConfig =
            toml::from_str(&content).map_err(|e| AgentError::Config(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), AgentError> {
        if self.core.poll_interval_secs == 0 {
            return Err(AgentError::Config(
                "poll_interval_secs must be greater than 0".to_string(),
            ));
        }
        if self.core.url.is_empty() {
            return Err(AgentError::Config("core.url cannot be empty".to_string()));
        }
        if self.core.api_key.is_empty() {
            return Err(AgentError::Config(
                "core.api_key cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --test config_test`
Expected: all 6 tests PASS

**Step 5: Commit**

```bash
git add src/config.rs tests/config_test.rs
git commit -m "feat: config.toml parsing with validation and defaults"
```

---

## Task 4: Core API Types

**Files:**
- Modify: `src/core_client.rs`

These structs mirror the Pydantic schemas in `core/src/ixforge/schemas/agent.py`.

**Step 1: Write Core API types**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// --- Responses from Core ---

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigResponse {
    pub config_hash: String,
    pub content: String,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusResponse {
    pub updated: u32,
    pub unchanged: u32,
    pub not_found: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatResponse {
    pub acknowledged: bool,
    pub config_hash_match: bool,
}

// --- Requests to Core ---

#[derive(Debug, Clone, Serialize)]
pub struct BgpSessionState {
    pub peer_ip: String,
    pub oper_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusReport {
    pub sessions: Vec<BgpSessionState>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BirdInstanceStatus {
    pub name: String,
    pub running: bool,
    pub uptime_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Heartbeat {
    pub version: String,
    pub uptime_seconds: f64,
    pub current_config_hash: String,
    pub bird_instances: Vec<BirdInstanceStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigApplied {
    pub config_hash: String,
}

// --- Core API error response ---

#[derive(Debug, Clone, Deserialize)]
pub struct CoreErrorDetail {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoreErrorResponse {
    pub error: CoreErrorDetail,
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles

**Step 3: Commit**

```bash
git add src/core_client.rs
git commit -m "feat: Core API request/response types matching Pydantic schemas"
```

---

## Task 5: Core API Client with Tests

**Files:**
- Modify: `src/core_client.rs` (add CoreClient struct + trait)
- Create: `tests/core_client_test.rs`

**Step 1: Write failing tests**

`tests/core_client_test.rs`:
```rust
use ixforge_agent::core_client::{
    BgpSessionState, ConfigApplied, CoreClient, Heartbeat, StatusReport,
    BirdInstanceStatus,
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

#[tokio::test]
async fn test_get_config_success() {
    let server = MockServer::start().await;
    let rs_id = "550e8400-e29b-41d4-a716-446655440000";

    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/route-servers/{rs_id}/agent/config"
        )))
        .and(header("X-API-Key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "config_hash": "abc123",
            "content": "router id 10.0.0.1;",
            "generated_at": "2024-01-15T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let client = CoreClient::new(
        &server.uri(),
        "test-key",
        &rs_id.parse().unwrap(),
        None,
    )
    .unwrap();

    let config = client.get_config().await.unwrap();
    assert_eq!(config.config_hash, "abc123");
    assert_eq!(config.content, "router id 10.0.0.1;");
}

#[tokio::test]
async fn test_get_config_not_found() {
    let server = MockServer::start().await;
    let rs_id = "550e8400-e29b-41d4-a716-446655440000";

    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/route-servers/{rs_id}/agent/config"
        )))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": {
                "code": "NOT_FOUND",
                "message": "No config exists"
            }
        })))
        .mount(&server)
        .await;

    let client = CoreClient::new(
        &server.uri(),
        "test-key",
        &rs_id.parse().unwrap(),
        None,
    )
    .unwrap();

    let result = client.get_config().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_report_status_success() {
    let server = MockServer::start().await;
    let rs_id = "550e8400-e29b-41d4-a716-446655440000";

    Mock::given(method("POST"))
        .and(path(format!(
            "/api/v1/route-servers/{rs_id}/agent/status"
        )))
        .and(header("X-API-Key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "updated": 1,
            "unchanged": 1,
            "not_found": 0
        })))
        .mount(&server)
        .await;

    let client = CoreClient::new(
        &server.uri(),
        "test-key",
        &rs_id.parse().unwrap(),
        None,
    )
    .unwrap();

    let report = StatusReport {
        sessions: vec![
            BgpSessionState {
                peer_ip: "10.0.0.1".to_string(),
                oper_state: "up".to_string(),
            },
            BgpSessionState {
                peer_ip: "10.0.0.2".to_string(),
                oper_state: "down".to_string(),
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
    let rs_id = "550e8400-e29b-41d4-a716-446655440000";

    Mock::given(method("POST"))
        .and(path(format!(
            "/api/v1/route-servers/{rs_id}/agent/heartbeat"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "acknowledged": true,
            "config_hash_match": true
        })))
        .mount(&server)
        .await;

    let client = CoreClient::new(
        &server.uri(),
        "test-key",
        &rs_id.parse().unwrap(),
        None,
    )
    .unwrap();

    let response = client.send_heartbeat(&test_heartbeat()).await.unwrap();
    assert!(response.acknowledged);
    assert!(response.config_hash_match);
}

#[tokio::test]
async fn test_heartbeat_upgrade_header() {
    let server = MockServer::start().await;
    let rs_id = "550e8400-e29b-41d4-a716-446655440000";

    Mock::given(method("POST"))
        .and(path(format!(
            "/api/v1/route-servers/{rs_id}/agent/heartbeat"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-IXForge-Agent-Upgrade", "0.2.0")
                .set_body_json(serde_json::json!({
                    "acknowledged": true,
                    "config_hash_match": false
                })),
        )
        .mount(&server)
        .await;

    let client = CoreClient::new(
        &server.uri(),
        "test-key",
        &rs_id.parse().unwrap(),
        None,
    )
    .unwrap();

    let (response, upgrade_version) =
        client.send_heartbeat_with_headers(&test_heartbeat()).await.unwrap();
    assert!(response.acknowledged);
    assert!(!response.config_hash_match);
    assert_eq!(upgrade_version.as_deref(), Some("0.2.0"));
}

#[tokio::test]
async fn test_confirm_config_applied() {
    let server = MockServer::start().await;
    let rs_id = "550e8400-e29b-41d4-a716-446655440000";

    Mock::given(method("POST"))
        .and(path(format!(
            "/api/v1/route-servers/{rs_id}/agent/config/applied"
        )))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = CoreClient::new(
        &server.uri(),
        "test-key",
        &rs_id.parse().unwrap(),
        None,
    )
    .unwrap();

    let body = ConfigApplied {
        config_hash: "a".repeat(64),
    };
    let result = client.confirm_config_applied(&body).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_core_unreachable() {
    let client = CoreClient::new(
        "http://127.0.0.1:1",
        "test-key",
        &"550e8400-e29b-41d4-a716-446655440000".parse().unwrap(),
        None,
    )
    .unwrap();

    let result = client.get_config().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_auth_rejected() {
    let server = MockServer::start().await;
    let rs_id = "550e8400-e29b-41d4-a716-446655440000";

    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/route-servers/{rs_id}/agent/config"
        )))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": {
                "code": "UNAUTHORIZED",
                "message": "Invalid API key"
            }
        })))
        .mount(&server)
        .await;

    let client = CoreClient::new(
        &server.uri(),
        "wrong-key",
        &rs_id.parse().unwrap(),
        None,
    )
    .unwrap();

    let result = client.get_config().await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("401") || err_msg.contains("UNAUTHORIZED"),
        "Error should mention auth failure: {err_msg}"
    );
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test core_client_test`
Expected: FAIL (CoreClient not implemented)

**Step 3: Implement CoreClient**

Add to `src/core_client.rs` (after the existing types):

```rust
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use uuid::Uuid;

use crate::error::AgentError;

pub struct CoreClient {
    http: reqwest::Client,
    base_url: String,
    route_server_id: Uuid,
    last_poll_ok: AtomicBool,
}

impl CoreClient {
    pub fn new(
        base_url: &str,
        api_key: &str,
        route_server_id: &Uuid,
        ca_cert_path: Option<&str>,
    ) -> Result<Self, AgentError> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "X-API-Key",
            api_key
                .parse()
                .map_err(|_| AgentError::Config("invalid API key characters".into()))?,
        );

        let mut builder = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30));

        if let Some(ca_path) = ca_cert_path {
            let ca_bytes = std::fs::read(ca_path)
                .map_err(|e| AgentError::io(ca_path, e))?;
            let cert = reqwest::Certificate::from_pem(&ca_bytes)
                .map_err(|e| AgentError::Config(format!("invalid CA certificate: {e}")))?;
            builder = builder.add_root_certificate(cert);
        }

        let http = builder
            .build()
            .map_err(|e| AgentError::Config(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            route_server_id: *route_server_id,
            last_poll_ok: AtomicBool::new(false),
        })
    }

    pub fn last_poll_ok(&self) -> bool {
        self.last_poll_ok.load(Ordering::Relaxed)
    }

    fn agent_url(&self, suffix: &str) -> String {
        format!(
            "{}/api/v1/route-servers/{}/agent{}",
            self.base_url, self.route_server_id, suffix
        )
    }

    pub async fn get_config(&self) -> Result<ConfigResponse, AgentError> {
        let resp = self
            .http
            .get(self.agent_url("/config"))
            .send()
            .await?;

        self.last_poll_ok.store(resp.status().is_success(), Ordering::Relaxed);

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentError::CoreApi(format!("{status}: {body}")));
        }

        resp.json().await.map_err(AgentError::Http)
    }

    pub async fn report_status(
        &self,
        report: &StatusReport,
    ) -> Result<StatusResponse, AgentError> {
        let resp = self
            .http
            .post(self.agent_url("/status"))
            .json(report)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentError::CoreApi(format!("{status}: {body}")));
        }

        resp.json().await.map_err(AgentError::Http)
    }

    pub async fn send_heartbeat(
        &self,
        heartbeat: &Heartbeat,
    ) -> Result<HeartbeatResponse, AgentError> {
        let (response, _) = self.send_heartbeat_with_headers(heartbeat).await?;
        Ok(response)
    }

    pub async fn send_heartbeat_with_headers(
        &self,
        heartbeat: &Heartbeat,
    ) -> Result<(HeartbeatResponse, Option<String>), AgentError> {
        let resp = self
            .http
            .post(self.agent_url("/heartbeat"))
            .json(heartbeat)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentError::CoreApi(format!("{status}: {body}")));
        }

        let upgrade_version = resp
            .headers()
            .get("X-IXForge-Agent-Upgrade")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let body = resp.json().await.map_err(AgentError::Http)?;
        Ok((body, upgrade_version))
    }

    pub async fn confirm_config_applied(
        &self,
        body: &ConfigApplied,
    ) -> Result<(), AgentError> {
        let resp = self
            .http
            .post(self.agent_url("/config/applied"))
            .json(body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(AgentError::CoreApi(format!("{status}: {body_text}")));
        }

        Ok(())
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --test core_client_test`
Expected: all 8 tests PASS

**Step 5: Commit**

```bash
git add src/core_client.rs tests/core_client_test.rs
git commit -m "feat: Core API client with auth, config polling, status, heartbeat"
```

---

## Task 6: BIRD Protocol Parser with Tests

**Files:**
- Modify: `src/bird/parser.rs`
- Create: `tests/bird_parser_test.rs`

This parses the output of `birdc show protocols all` (BIRD 2.x format).

**Step 1: Write the failing tests**

`tests/bird_parser_test.rs`:
```rust
use ixforge_agent::bird::parser::{parse_protocols, BirdProtocol, BgpState};

const BIRD_OUTPUT_MIXED: &str = r#"BIRD 2.15.1 ready.
Name       Proto      Table    State  Since       Info
device1    Device     ---      up     2024-01-15

direct1    Direct     ---      up     2024-01-15

peer_as64500 BGP        ---      up     2024-01-15  Established
  Description:    Peer AS64500
  BGP state:          Established
    Neighbor address: 10.0.0.1
    Neighbor AS:      64500
    Local AS:         65000

  Channel ipv4
    State:          UP
    Table:          master4
    Preference:     100
    Input filter:   ACCEPT
    Output filter:  ACCEPT
    Routes:         42 imported, 15 exported, 42 preferred

peer_as64501 BGP        ---      start  2024-01-15  Active
  Description:    Peer AS64501
  BGP state:          Active
    Neighbor address: 10.0.0.2
    Neighbor AS:      64501
    Local AS:         65000

  Channel ipv4
    State:          DOWN
    Table:          master4
    Preference:     100
    Input filter:   ACCEPT
    Output filter:  ACCEPT
    Routes:         0 imported, 0 exported, 0 preferred

peer_as64502_v6 BGP        ---      up     2024-01-14  Established
  Description:    Peer AS64502 IPv6
  BGP state:          Established
    Neighbor address: 2001:db8::1
    Neighbor AS:      64502
    Local AS:         65000

  Channel ipv6
    State:          UP
    Table:          master6
    Preference:     100
    Input filter:   ACCEPT
    Output filter:  ACCEPT
    Routes:         100 imported, 50 exported, 100 preferred
"#;

#[test]
fn test_parse_protocols_extracts_bgp_only() {
    let protocols = parse_protocols(BIRD_OUTPUT_MIXED).unwrap();
    // Should only include BGP protocols, not Device or Direct
    assert_eq!(protocols.len(), 3);
    assert!(protocols.iter().all(|p| p.proto == "BGP"));
}

#[test]
fn test_parse_established_session() {
    let protocols = parse_protocols(BIRD_OUTPUT_MIXED).unwrap();
    let p = protocols.iter().find(|p| p.name == "peer_as64500").unwrap();

    assert_eq!(p.state, BgpState::Up);
    assert_eq!(p.neighbor_address.as_deref(), Some("10.0.0.1"));
    assert_eq!(p.neighbor_asn, Some(64500));
    assert_eq!(p.prefixes_imported, Some(42));
    assert_eq!(p.prefixes_exported, Some(15));
}

#[test]
fn test_parse_active_session() {
    let protocols = parse_protocols(BIRD_OUTPUT_MIXED).unwrap();
    let p = protocols.iter().find(|p| p.name == "peer_as64501").unwrap();

    assert_eq!(p.state, BgpState::Down);
    assert_eq!(p.neighbor_address.as_deref(), Some("10.0.0.2"));
    assert_eq!(p.neighbor_asn, Some(64501));
    assert_eq!(p.prefixes_imported, Some(0));
    assert_eq!(p.prefixes_exported, Some(0));
}

#[test]
fn test_parse_ipv6_session() {
    let protocols = parse_protocols(BIRD_OUTPUT_MIXED).unwrap();
    let p = protocols
        .iter()
        .find(|p| p.name == "peer_as64502_v6")
        .unwrap();

    assert_eq!(p.state, BgpState::Up);
    assert_eq!(p.neighbor_address.as_deref(), Some("2001:db8::1"));
    assert_eq!(p.neighbor_asn, Some(64502));
    assert_eq!(p.prefixes_imported, Some(100));
    assert_eq!(p.prefixes_exported, Some(50));
}

#[test]
fn test_parse_empty_output() {
    let protocols =
        parse_protocols("BIRD 2.15.1 ready.\nName       Proto      Table    State  Since       Info\n")
            .unwrap();
    assert!(protocols.is_empty());
}

#[test]
fn test_bgp_state_mapping() {
    assert_eq!(BgpState::from_bird_info("Established"), BgpState::Up);
    assert_eq!(BgpState::from_bird_info("Active"), BgpState::Down);
    assert_eq!(BgpState::from_bird_info("Connect"), BgpState::Down);
    assert_eq!(BgpState::from_bird_info("OpenSent"), BgpState::Down);
    assert_eq!(BgpState::from_bird_info("OpenConfirm"), BgpState::Down);
    assert_eq!(BgpState::from_bird_info("Idle"), BgpState::Down);
    assert_eq!(BgpState::from_bird_info("SomethingWeird"), BgpState::Unknown);
}

#[test]
fn test_bgp_state_to_oper_state_string() {
    assert_eq!(BgpState::Up.as_oper_state(), "up");
    assert_eq!(BgpState::Down.as_oper_state(), "down");
    assert_eq!(BgpState::Unknown.as_oper_state(), "unknown");
}

const BIRD_OUTPUT_CONNECT_STATES: &str = r#"BIRD 2.15.1 ready.
Name       Proto      Table    State  Since       Info
peer_idle  BGP        ---      start  2024-01-15  Idle
  BGP state:          Idle
    Neighbor address: 10.0.0.10
    Neighbor AS:      64510
    Local AS:         65000

peer_opensent BGP        ---      start  2024-01-15  OpenSent
  BGP state:          OpenSent
    Neighbor address: 10.0.0.11
    Neighbor AS:      64511
    Local AS:         65000

peer_openconfirm BGP        ---      start  2024-01-15  OpenConfirm
  BGP state:          OpenConfirm
    Neighbor address: 10.0.0.12
    Neighbor AS:      64512
    Local AS:         65000
"#;

#[test]
fn test_parse_various_down_states() {
    let protocols = parse_protocols(BIRD_OUTPUT_CONNECT_STATES).unwrap();
    assert_eq!(protocols.len(), 3);
    assert!(protocols.iter().all(|p| p.state == BgpState::Down));
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test bird_parser_test`
Expected: FAIL (parser module is empty)

**Step 3: Implement the parser**

`src/bird/parser.rs`:
```rust
use crate::error::AgentError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BgpState {
    Up,
    Down,
    Unknown,
}

impl BgpState {
    pub fn from_bird_info(info: &str) -> Self {
        match info.trim() {
            "Established" => Self::Up,
            "Active" | "Connect" | "OpenSent" | "OpenConfirm" | "Idle" => Self::Down,
            _ => Self::Unknown,
        }
    }

    pub fn as_oper_state(&self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BirdProtocol {
    pub name: String,
    pub proto: String,
    pub state: BgpState,
    pub neighbor_address: Option<String>,
    pub neighbor_asn: Option<u32>,
    pub prefixes_imported: Option<u32>,
    pub prefixes_exported: Option<u32>,
}

pub fn parse_protocols(output: &str) -> Result<Vec<BirdProtocol>, AgentError> {
    let mut protocols = Vec::new();
    let mut current: Option<ProtocolBuilder> = None;

    for line in output.lines() {
        // Skip the BIRD ready line and header
        if line.starts_with("BIRD ") || line.starts_with("Name ") || line.is_empty() {
            // If we hit an empty line and have a current protocol, finalize it
            if line.is_empty() {
                if let Some(builder) = current.take() {
                    if let Some(proto) = builder.build() {
                        protocols.push(proto);
                    }
                }
            }
            continue;
        }

        // New protocol line: starts with a non-whitespace character
        if !line.starts_with(' ') && !line.starts_with('\t') {
            // Finalize previous protocol
            if let Some(builder) = current.take() {
                if let Some(proto) = builder.build() {
                    protocols.push(proto);
                }
            }
            current = ProtocolBuilder::from_header(line);
            continue;
        }

        // Detail line for current protocol
        if let Some(ref mut builder) = current {
            builder.parse_detail(line);
        }
    }

    // Finalize last protocol
    if let Some(builder) = current.take() {
        if let Some(proto) = builder.build() {
            protocols.push(proto);
        }
    }

    Ok(protocols)
}

struct ProtocolBuilder {
    name: String,
    proto: String,
    info: String,
    neighbor_address: Option<String>,
    neighbor_asn: Option<u32>,
    prefixes_imported: Option<u32>,
    prefixes_exported: Option<u32>,
}

impl ProtocolBuilder {
    fn from_header(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            return None;
        }

        let name = parts[0].to_string();
        let proto = parts[1].to_string();
        let info = if parts.len() >= 6 {
            parts[5..].join(" ")
        } else {
            String::new()
        };

        Some(Self {
            name,
            proto,
            info,
            neighbor_address: None,
            neighbor_asn: None,
            prefixes_imported: None,
            prefixes_exported: None,
        })
    }

    fn parse_detail(&mut self, line: &str) {
        let trimmed = line.trim();

        if let Some(addr) = trimmed.strip_prefix("Neighbor address:") {
            self.neighbor_address = Some(addr.trim().to_string());
        } else if let Some(asn_str) = trimmed.strip_prefix("Neighbor AS:") {
            self.neighbor_asn = asn_str.trim().parse().ok();
        } else if let Some(bgp_state) = trimmed.strip_prefix("BGP state:") {
            // Use BGP state from detail section (more reliable than header info)
            self.info = bgp_state.trim().to_string();
        } else if trimmed.starts_with("Routes:") {
            self.parse_routes(trimmed);
        }
    }

    fn parse_routes(&mut self, line: &str) {
        // "Routes:         42 imported, 15 exported, 42 preferred"
        if let Some(routes_part) = line.strip_prefix("Routes:") {
            let parts: Vec<&str> = routes_part.split(',').collect();
            for part in parts {
                let tokens: Vec<&str> = part.trim().split_whitespace().collect();
                if tokens.len() >= 2 {
                    if let Ok(count) = tokens[0].parse::<u32>() {
                        match tokens[1] {
                            "imported" => self.prefixes_imported = Some(count),
                            "exported" => self.prefixes_exported = Some(count),
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    fn build(self) -> Option<BirdProtocol> {
        // Only include BGP protocols
        if self.proto != "BGP" {
            return None;
        }

        Some(BirdProtocol {
            name: self.name,
            proto: self.proto,
            state: BgpState::from_bird_info(&self.info),
            neighbor_address: self.neighbor_address,
            neighbor_asn: self.neighbor_asn,
            prefixes_imported: self.prefixes_imported,
            prefixes_exported: self.prefixes_exported,
        })
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --test bird_parser_test`
Expected: all 9 tests PASS

**Step 5: Commit**

```bash
git add src/bird/parser.rs tests/bird_parser_test.rs
git commit -m "feat: BIRD 2.x protocol parser for BGP state and prefix counts"
```

---

## Task 7: BIRD Client Trait + Socket Implementation

**Files:**
- Modify: `src/bird/mod.rs`
- Modify: `src/bird/socket.rs`
- Modify: `src/bird/manager.rs`

**Step 1: Define the BirdClient trait**

`src/bird/mod.rs`:
```rust
pub mod manager;
pub mod parser;
pub mod socket;

use crate::bird::parser::BirdProtocol;
use crate::error::AgentError;

/// Trait abstracting BIRD interaction for testability
pub trait BirdClient: Send + Sync {
    /// Send a command to BIRD via control socket and return the response
    fn send_command(
        &self,
        command: &str,
    ) -> impl std::future::Future<Output = Result<String, AgentError>> + Send;

    /// Check if BIRD is running and responsive
    fn is_running(
        &self,
    ) -> impl std::future::Future<Output = bool> + Send;
}
```

**Step 2: Implement the real socket client**

`src/bird/socket.rs`:
```rust
use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::error::AgentError;
use super::BirdClient;

pub struct BirdSocketClient {
    socket_path: PathBuf,
}

impl BirdSocketClient {
    pub fn new(socket_path: &str) -> Self {
        Self {
            socket_path: PathBuf::from(socket_path),
        }
    }
}

impl BirdClient for BirdSocketClient {
    async fn send_command(&self, command: &str) -> Result<String, AgentError> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| AgentError::BirdSocket(format!(
                "failed to connect to {}: {e}", self.socket_path.display()
            )))?;

        // Read the welcome banner
        let mut banner = vec![0u8; 4096];
        let n = stream.read(&mut banner).await
            .map_err(|e| AgentError::BirdSocket(format!("failed to read banner: {e}")))?;
        let _banner_str = String::from_utf8_lossy(&banner[..n]);

        // Send the command
        let cmd = format!("{command}\n");
        stream.write_all(cmd.as_bytes()).await
            .map_err(|e| AgentError::BirdSocket(format!("failed to send command: {e}")))?;

        // Read the full response
        let mut response = String::new();
        let mut buf = vec![0u8; 8192];
        loop {
            let n = stream.read(&mut buf).await
                .map_err(|e| AgentError::BirdSocket(format!("failed to read response: {e}")))?;
            if n == 0 {
                break;
            }
            response.push_str(&String::from_utf8_lossy(&buf[..n]));

            // BIRD protocol: lines starting with a 4-digit code and space (not '-') indicate end
            // e.g., "0000 " = ok, "8001 " = error, etc.
            if response.lines().last().is_some_and(|line| {
                line.len() >= 5
                    && line.as_bytes()[4] == b' '
                    && line[..4].chars().all(|c| c.is_ascii_digit())
            }) {
                break;
            }
        }

        Ok(response)
    }

    async fn is_running(&self) -> bool {
        UnixStream::connect(&self.socket_path).await.is_ok()
    }
}
```

**Step 3: Implement the manager (validate, apply, get protocols)**

`src/bird/manager.rs`:
```rust
use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::{info, error, warn};

use crate::bird::parser::{parse_protocols, BirdProtocol};
use crate::error::AgentError;
use super::BirdClient;

pub struct BirdManager<C: BirdClient> {
    client: C,
    config_path: PathBuf,
    bird_binary: PathBuf,
}

impl<C: BirdClient> BirdManager<C> {
    pub fn new(client: C, config_path: &str, bird_binary: &str) -> Self {
        Self {
            client,
            config_path: PathBuf::from(config_path),
            bird_binary: PathBuf::from(bird_binary),
        }
    }

    /// Validate a config file using `bird -p -c <path>`
    pub async fn validate_config(&self, temp_config_path: &Path) -> Result<(), AgentError> {
        let output = Command::new(&self.bird_binary)
            .args(["-p", "-c"])
            .arg(temp_config_path)
            .output()
            .await
            .map_err(|e| AgentError::BirdValidation(format!(
                "failed to run {}: {e}", self.bird_binary.display()
            )))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(AgentError::BirdValidation(format!(
                "bird -p failed (exit {}): {stderr} {stdout}",
                output.status
            )));
        }

        Ok(())
    }

    /// Apply config by sending `configure` to BIRD via socket
    pub async fn apply_config(&self) -> Result<(), AgentError> {
        let response = self.client.send_command("configure").await?;

        // BIRD responds with "0003-Reading configuration from ..."
        // and "0004 Reconfigured" on success
        if response.contains("Reconfigured") {
            info!(config_path = %self.config_path.display(), "BIRD config applied");
            Ok(())
        } else {
            Err(AgentError::BirdCommand(format!(
                "configure failed: {response}"
            )))
        }
    }

    /// Get all BGP protocol states
    pub async fn get_protocols(&self) -> Result<Vec<BirdProtocol>, AgentError> {
        let output = self.client.send_command("show protocols all").await?;
        parse_protocols(&output)
    }

    /// Check if BIRD is running
    pub async fn is_running(&self) -> bool {
        self.client.is_running().await
    }

    /// Write config content to the config file
    pub async fn write_config(&self, content: &str) -> Result<(), AgentError> {
        tokio::fs::write(&self.config_path, content)
            .await
            .map_err(|e| AgentError::io(&self.config_path, e))
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }
}
```

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: compiles

**Step 5: Commit**

```bash
git add src/bird/
git commit -m "feat: BirdClient trait, socket impl, and manager for validate/apply/status"
```

---

## Task 8: Agent State

**Files:**
- Modify: `src/state.rs`

**Step 1: Implement agent state**

```rust
use std::time::Instant;

use crate::bird::parser::BirdProtocol;

pub struct AgentState {
    pub current_config_hash: String,
    pub started_at: Instant,
    pub last_protocols: Vec<BirdProtocol>,
    pub poll_errors: u64,
}

impl AgentState {
    pub fn new() -> Self {
        Self {
            current_config_hash: String::new(),
            started_at: Instant::now(),
            last_protocols: Vec::new(),
            poll_errors: 0,
        }
    }

    pub fn uptime_seconds(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    pub fn has_config(&self) -> bool {
        !self.current_config_hash.is_empty()
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles

**Step 3: Commit**

```bash
git add src/state.rs
git commit -m "feat: agent internal state tracking"
```

---

## Task 9: Logging Setup

**Files:**
- Create: `src/logging.rs`
- Modify: `src/lib.rs` (add `pub mod logging;`)

**Step 1: Implement logging initialization**

`src/logging.rs`:
```rust
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

use crate::config::LoggingConfig;

/// Initialize tracing with stdout + optional file output.
/// Returns guards that must be held for the lifetime of the program
/// (dropping them flushes pending writes).
pub fn init(config: &LoggingConfig) -> Vec<WorkerGuard> {
    let mut guards = Vec::new();

    let filter = EnvFilter::try_new(&config.level)
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let is_json = config.format == "json";

    if let Some(ref file_path) = config.file_path {
        // File + stdout dual output
        let file_appender = tracing_appender::rolling::never(
            std::path::Path::new(file_path)
                .parent()
                .unwrap_or(std::path::Path::new(".")),
            std::path::Path::new(file_path)
                .file_name()
                .unwrap_or(std::ffi::OsStr::new("ixforge-agent.log")),
        );
        let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);
        guards.push(file_guard);

        let (stdout_writer, stdout_guard) = tracing_appender::non_blocking(std::io::stdout());
        guards.push(stdout_guard);

        if is_json {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().json().with_writer(stdout_writer))
                .with(fmt::layer().json().with_writer(file_writer))
                .init();
        } else {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().with_writer(stdout_writer))
                .with(fmt::layer().with_writer(file_writer))
                .init();
        }
    } else {
        // Stdout only
        let (stdout_writer, stdout_guard) = tracing_appender::non_blocking(std::io::stdout());
        guards.push(stdout_guard);

        if is_json {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().json().with_writer(stdout_writer))
                .init();
        } else {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().with_writer(stdout_writer))
                .init();
        }
    }

    guards
}
```

**Step 2: Add to lib.rs**

Add `pub mod logging;` to `src/lib.rs`.

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles

**Step 4: Commit**

```bash
git add src/logging.rs src/lib.rs
git commit -m "feat: structured logging with stdout + optional file output"
```

---

## Task 10: Main Loop (Config Polling + BIRD Apply + Status + Heartbeat)

**Files:**
- Modify: `src/main.rs`

**Step 1: Implement the full main loop**

```rust
use std::path::Path;
use std::process;

use clap::Parser;
use sha2::{Digest, Sha256};
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

use ixforge_agent::bird::manager::BirdManager;
use ixforge_agent::bird::socket::BirdSocketClient;
use ixforge_agent::config::AgentConfig;
use ixforge_agent::core_client::{
    BgpSessionState, BirdInstanceStatus, ConfigApplied, CoreClient,
    Heartbeat, StatusReport,
};
use ixforge_agent::state::AgentState;

#[derive(Parser)]
#[command(name = "ixforge-agent", version, about = "IXForge route server agent")]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "/etc/ixforge-agent/config.toml")]
    config: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Load config
    let config = match AgentConfig::from_file(Path::new(&cli.config)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config from {}: {e}", cli.config);
            process::exit(1);
        }
    };

    // Initialize logging (guards must live for program lifetime)
    let _log_guards = ixforge_agent::logging::init(&config.logging);

    info!(
        version = env!("CARGO_PKG_VERSION"),
        config_path = %cli.config,
        core_url = %config.core.url,
        route_server_id = %config.core.route_server_id,
        poll_interval = config.core.poll_interval_secs,
        "ixforge-agent starting"
    );

    // Initialize components
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

    let bird_socket = BirdSocketClient::new(&config.bird.socket_path);
    let bird_manager = BirdManager::new(
        bird_socket,
        &config.bird.config_path,
        &config.bird.bird_binary,
    );

    let mut state = AgentState::new();

    // Start metrics server in background
    let metrics_listen = config.metrics.listen.clone();
    let metrics_core_client_ok = {
        // We'll share state via the metrics server later in Task 11
        // For now, just spawn placeholder
        tokio::spawn(async move {
            ixforge_agent::metrics::server::run(&metrics_listen).await;
        })
    };

    let interval = Duration::from_secs(config.core.poll_interval_secs);

    info!("entering main polling loop");

    loop {
        // --- Step 1: Config polling ---
        match core_client.get_config().await {
            Ok(config_resp) => {
                if config_resp.config_hash != state.current_config_hash {
                    info!(
                        new_hash = %config_resp.config_hash,
                        old_hash = %state.current_config_hash,
                        "new config detected"
                    );

                    // Write to temp file for validation
                    let temp_path = format!("{}.tmp", config.bird.config_path);
                    match tokio::fs::write(&temp_path, &config_resp.content).await {
                        Ok(()) => {}
                        Err(e) => {
                            error!(
                                error = %e,
                                path = %temp_path,
                                "failed to write temp config file"
                            );
                            state.poll_errors += 1;
                            sleep(interval).await;
                            continue;
                        }
                    }

                    // Validate with bird -p
                    match bird_manager.validate_config(Path::new(&temp_path)).await {
                        Ok(()) => {
                            info!("config validation passed");

                            // Write final config
                            if let Err(e) = bird_manager.write_config(&config_resp.content).await {
                                error!(error = %e, "failed to write config");
                                state.poll_errors += 1;
                                sleep(interval).await;
                                continue;
                            }

                            // Apply via birdc configure
                            match bird_manager.apply_config().await {
                                Ok(()) => {
                                    state.current_config_hash =
                                        config_resp.config_hash.clone();

                                    // Confirm to Core
                                    let applied = ConfigApplied {
                                        config_hash: config_resp.config_hash,
                                    };
                                    if let Err(e) =
                                        core_client.confirm_config_applied(&applied).await
                                    {
                                        warn!(
                                            error = %e,
                                            "failed to confirm config applied"
                                        );
                                    }
                                }
                                Err(e) => {
                                    error!(error = %e, "failed to apply config via birdc");
                                    state.poll_errors += 1;
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                error = %e,
                                "config validation failed, keeping previous config"
                            );
                            state.poll_errors += 1;
                        }
                    }

                    // Clean up temp file (best effort)
                    let _ = tokio::fs::remove_file(&temp_path).await;
                }
            }
            Err(e) => {
                warn!(error = %e, "failed to poll config from Core");
                state.poll_errors += 1;
            }
        }

        // --- Step 2: BGP status reporting ---
        match bird_manager.get_protocols().await {
            Ok(protocols) => {
                let sessions: Vec<BgpSessionState> = protocols
                    .iter()
                    .filter_map(|p| {
                        p.neighbor_address.as_ref().map(|addr| BgpSessionState {
                            peer_ip: addr.clone(),
                            oper_state: p.state.as_oper_state().to_string(),
                        })
                    })
                    .collect();

                if !sessions.is_empty() {
                    let report = StatusReport { sessions };
                    if let Err(e) = core_client.report_status(&report).await {
                        warn!(error = %e, "failed to report BGP status");
                    }
                }

                state.last_protocols = protocols;
            }
            Err(e) => {
                warn!(error = %e, "failed to get BIRD protocols");
            }
        }

        // --- Step 3: Heartbeat ---
        let bird_running = bird_manager.is_running().await;
        let heartbeat = Heartbeat {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: state.uptime_seconds(),
            current_config_hash: if state.has_config() {
                state.current_config_hash.clone()
            } else {
                // Send dummy hash if no config yet
                "0".repeat(64)
            },
            bird_instances: vec![BirdInstanceStatus {
                name: "bird".to_string(),
                running: bird_running,
                uptime_seconds: None,
            }],
        };

        match core_client.send_heartbeat_with_headers(&heartbeat).await {
            Ok((response, upgrade_version)) => {
                if !response.config_hash_match && state.has_config() {
                    warn!("config hash mismatch reported by Core");
                }
                if let Some(version) = upgrade_version {
                    warn!(
                        required_version = %version,
                        "Core requests agent upgrade"
                    );
                }
            }
            Err(e) => {
                warn!(error = %e, "failed to send heartbeat");
            }
        }

        sleep(interval).await;
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles (metrics server placeholder needed, see Task 11)

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: main polling loop with config apply, BGP status, and heartbeat"
```

---

## Task 11: Metrics HTTP Server + Health Endpoint

**Files:**
- Modify: `src/metrics/server.rs`
- Modify: `src/metrics/registry.rs`
- Modify: `src/metrics/system.rs`
- Create: `tests/health_test.rs`

**Step 1: Write health endpoint test**

`tests/health_test.rs`:
```rust
use serde_json::Value;

#[tokio::test]
async fn test_health_endpoint_returns_json() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        ixforge_agent::metrics::server::run_with_listener(listener).await;
    });

    // Give server time to start
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
    // Should contain at least the agent uptime metric
    assert!(
        body.contains("ixforge_agent_uptime_seconds"),
        "metrics should contain agent uptime: {body}"
    );

    server_handle.abort();
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test health_test`
Expected: FAIL (metrics server not implemented)

**Step 3: Implement system metrics**

`src/metrics/system.rs`:
```rust
use sysinfo::System;

pub struct SystemMetrics {
    system: System,
}

impl SystemMetrics {
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
        }
    }

    pub fn refresh(&mut self) {
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
    }

    pub fn cpu_usage(&self) -> f64 {
        self.system.global_cpu_usage() as f64
    }

    pub fn memory_usage_percent(&self) -> f64 {
        let total = self.system.total_memory() as f64;
        let used = self.system.used_memory() as f64;
        if total > 0.0 {
            (used / total) * 100.0
        } else {
            0.0
        }
    }
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 4: Implement metrics registry**

`src/metrics/registry.rs`:
```rust
use std::sync::{Arc, Mutex};
use std::time::Instant;

use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;

use super::system::SystemMetrics;

pub struct MetricsRegistry {
    registry: Registry,
    pub agent_uptime: Gauge,
    pub poll_errors_total: Counter,
    pub bgp_sessions_up: Gauge,
    pub bgp_sessions_total: Gauge,
    pub host_cpu_usage: Gauge<f64, prometheus_client::metrics::gauge::Atomic<f64>>,
    pub host_memory_usage: Gauge<f64, prometheus_client::metrics::gauge::Atomic<f64>>,
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

        Arc::new(Self {
            registry,
            agent_uptime,
            poll_errors_total,
            bgp_sessions_up,
            bgp_sessions_total,
            host_cpu_usage,
            host_memory_usage,
            started_at: Instant::now(),
            system_metrics: Mutex::new(SystemMetrics::new()),
        })
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
        buf
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        // Can't return Arc from Default, but this satisfies the trait
        unreachable!("use MetricsRegistry::new() instead")
    }
}
```

**Step 5: Implement the HTTP server**

`src/metrics/server.rs`:
```rust
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use serde_json::json;
use tokio::net::TcpListener;

use super::registry::MetricsRegistry;

#[derive(Clone)]
struct AppState {
    metrics: Arc<MetricsRegistry>,
    core_connected: Arc<std::sync::atomic::AtomicBool>,
    bird_running: Arc<std::sync::atomic::AtomicBool>,
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let metrics = &state.metrics;
    metrics.refresh();

    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": metrics.agent_uptime.get(),
        "bird": {
            "running": state.bird_running.load(std::sync::atomic::Ordering::Relaxed),
        },
        "core_connected": state.core_connected.load(std::sync::atomic::Ordering::Relaxed),
    }))
}

async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let body = state.metrics.encode();
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(state)
}

/// Run the metrics server on the given address string
pub async fn run(listen: &str) {
    let listener = match TcpListener::bind(listen).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, listen = %listen, "failed to bind metrics server");
            return;
        }
    };
    run_with_listener(listener).await;
}

/// Run the metrics server with a pre-bound listener (for testing)
pub async fn run_with_listener(listener: TcpListener) {
    let state = AppState {
        metrics: MetricsRegistry::new(),
        core_connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        bird_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    let app = build_router(state);

    tracing::info!(
        addr = %listener.local_addr().unwrap(),
        "metrics server listening"
    );

    axum::serve(listener, app).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "metrics server error");
    });
}

/// Run the metrics server with shared state from the main loop
pub async fn run_with_state(
    listener: TcpListener,
    metrics: Arc<MetricsRegistry>,
    core_connected: Arc<std::sync::atomic::AtomicBool>,
    bird_running: Arc<std::sync::atomic::AtomicBool>,
) {
    let state = AppState {
        metrics,
        core_connected,
        bird_running,
    };

    let app = build_router(state);

    tracing::info!(
        addr = %listener.local_addr().unwrap(),
        "metrics server listening"
    );

    axum::serve(listener, app).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "metrics server error");
    });
}
```

**Step 6: Run tests to verify they pass**

Run: `cargo test --test health_test`
Expected: all 2 tests PASS

**Step 7: Commit**

```bash
git add src/metrics/ tests/health_test.rs
git commit -m "feat: Prometheus metrics server with /health and /metrics endpoints"
```

---

## Task 12: Wire Metrics into Main Loop

**Files:**
- Modify: `src/main.rs`

**Step 1: Update main.rs to use shared metrics state**

Replace the metrics server spawn section and add shared state updates in the polling loop. The key changes:

1. Create `Arc<MetricsRegistry>` and shared `AtomicBool`s for `core_connected` and `bird_running`
2. Spawn `run_with_state()` with the shared state
3. Update `core_connected` after each poll attempt
4. Update `bird_running` after each BIRD check
5. Update `bgp_sessions_up` and `bgp_sessions_total` after protocol parsing
6. Increment `poll_errors_total` on errors

The main loop structure stays the same, but now the metrics server reflects live state.

See Task 10 for the base structure. Key additions to the loop body:

```rust
// After config poll success/failure
core_connected.store(
    core_client.last_poll_ok(),
    std::sync::atomic::Ordering::Relaxed,
);

// After BIRD protocol parsing
bird_running_flag.store(bird_running, std::sync::atomic::Ordering::Relaxed);
let up_count = state.last_protocols.iter()
    .filter(|p| p.state == ixforge_agent::bird::parser::BgpState::Up)
    .count();
metrics.bgp_sessions_up.set(up_count as i64);
metrics.bgp_sessions_total.set(state.last_protocols.len() as i64);

// On errors
metrics.poll_errors_total.inc();
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire shared metrics state into main polling loop"
```

---

## Task 13: Integration Test Setup (BIRD in Docker)

**Files:**
- Create: `tests/integration/mod.rs`
- Create: `tests/integration/bird_test.rs`
- Create: `docker/bird/Dockerfile`
- Create: `docker/bird/bird.conf`
- Create: `docker/docker-compose.test.yml`

**Step 1: Create BIRD Docker setup for integration tests**

`docker/bird/Dockerfile`:
```dockerfile
FROM alpine:3.20
RUN apk add --no-cache bird
COPY bird.conf /etc/bird/bird.conf
CMD ["bird", "-f", "-c", "/etc/bird/bird.conf"]
```

`docker/bird/bird.conf`:
```
log syslog all;
router id 10.0.0.254;

protocol device {
}

protocol direct {
    ipv4;
}

protocol kernel {
    ipv4 {
        export all;
    };
}
```

`docker/docker-compose.test.yml`:
```yaml
services:
  bird:
    build:
      context: ./bird
    volumes:
      - bird-socket:/run/bird
    privileged: true

volumes:
  bird-socket:
```

**Step 2: Create integration test**

`tests/integration/mod.rs`:
```rust
// Integration tests require BIRD running in Docker
// Run: docker compose -f docker/docker-compose.test.yml up -d
// Then: cargo test --test integration -- --ignored
```

`tests/integration/bird_test.rs`:
```rust
use ixforge_agent::bird::socket::BirdSocketClient;
use ixforge_agent::bird::BirdClient;

#[tokio::test]
#[ignore = "requires BIRD running in Docker"]
async fn test_bird_socket_connection() {
    let client = BirdSocketClient::new("/run/bird/bird.ctl");
    assert!(client.is_running().await, "BIRD should be running");
}

#[tokio::test]
#[ignore = "requires BIRD running in Docker"]
async fn test_bird_show_protocols() {
    let client = BirdSocketClient::new("/run/bird/bird.ctl");
    let output = client.send_command("show protocols all").await.unwrap();
    assert!(output.contains("device1"), "should list device protocol");
}
```

**Step 3: Commit**

```bash
git add tests/integration/ docker/
git commit -m "feat: BIRD Docker setup and integration test scaffolding"
```

---

## Task 14: Systemd Unit File + Documentation

**Files:**
- Create: `ixforge-agent.service`
- Modify: `README.md`

**Step 1: Create systemd unit file**

`ixforge-agent.service`:
```ini
[Unit]
Description=IXForge Route Server Agent
Documentation=https://github.com/ixforge/agent
After=network-online.target bird.service
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/ixforge-agent --config /etc/ixforge-agent/config.toml
Restart=always
RestartSec=5
User=root

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/etc/bird
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
```

**Step 2: Update README.md**

Replace the existing README with project documentation covering:
- What the agent does
- Prerequisites (BIRD 2.x, IXForge Core running)
- Installation (download binary, create config, enable service)
- Configuration reference (all config.toml options)
- Metrics endpoint reference
- Health endpoint reference
- Troubleshooting

**Step 3: Commit**

```bash
git add ixforge-agent.service README.md
git commit -m "docs: systemd unit file and README with installation guide"
```

---

## Task 15: CI Setup

**Files:**
- Create: `.github/workflows/ci.yml`

**Step 1: Create GitHub Actions CI**

`.github/workflows/ci.yml`:
```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    name: Check + Clippy + Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2

      - name: Check
        run: cargo check --all-targets

      - name: Clippy
        run: cargo clippy --all-targets -- -D warnings

      - name: Tests
        run: cargo test

  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check

  build:
    name: Build (linux/amd64)
    runs-on: ubuntu-latest
    needs: [check, fmt]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Build release
        run: cargo build --release

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ixforge-agent-linux-amd64
          path: target/release/ixforge-agent
```

**Step 2: Commit**

```bash
git add .github/
git commit -m "ci: GitHub Actions with check, clippy, tests, fmt, and release build"
```

---

## Summary

| Task | What it builds | Tests |
|------|---------------|-------|
| 1 | Project scaffold, Cargo.toml, module structure | Compiles |
| 2 | Error types | Compiles |
| 3 | Config parsing + validation | 6 unit tests |
| 4 | Core API request/response types | Compiles |
| 5 | Core API HTTP client | 8 tests (wiremock) |
| 6 | BIRD protocol parser | 9 unit tests |
| 7 | BirdClient trait + socket impl + manager | Compiles |
| 8 | Agent state tracking | Compiles |
| 9 | Logging (stdout + file) | Compiles |
| 10 | Main loop (poll + apply + status + heartbeat) | Compiles |
| 11 | Metrics HTTP server + /health + /metrics | 2 integration tests |
| 12 | Wire metrics into main loop | Compiles |
| 13 | BIRD Docker + integration test scaffolding | Ignored integration tests |
| 14 | Systemd unit + README | Documentation |
| 15 | CI (GitHub Actions) | CI pipeline |

**Total estimated tests: 25+ unit/integration tests**

After completing all 15 tasks, the agent can:
- Poll config from Core, validate with `bird -p`, apply via `birdc configure`
- Report BGP session states (including prefix counts) to Core
- Send heartbeats with version, uptime, config hash, BIRD instance status
- Handle upgrade header from Core
- Expose Prometheus metrics (BGP sessions, prefixes, agent uptime, poll errors, host cpu/memory)
- Serve /health with version, uptime, BIRD status, Core connectivity
- Log structured JSON to stdout + optional file
- Run as a systemd service
