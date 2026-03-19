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
    pub af: u8,
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

// --- HTTP Client ---

use crate::error::AgentError;
use uuid::Uuid;

pub struct CoreClient {
    http: reqwest::Client,
    base_url: String,
    route_server_id: Uuid,
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
            let ca_bytes = std::fs::read(ca_path).map_err(|e| AgentError::io(ca_path, e))?;
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
        })
    }

    fn agent_url(&self, suffix: &str) -> String {
        format!(
            "{}/api/v1/route-servers/{}/agent{}",
            self.base_url, self.route_server_id, suffix
        )
    }

    pub async fn get_config(&self) -> Result<ConfigResponse, AgentError> {
        let resp = self.http.get(self.agent_url("/config")).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentError::CoreApi(format!("{status}: {body}")));
        }
        resp.json().await.map_err(AgentError::Http)
    }

    pub async fn report_status(&self, report: &StatusReport) -> Result<StatusResponse, AgentError> {
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

    pub async fn confirm_config_applied(&self, body: &ConfigApplied) -> Result<(), AgentError> {
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
