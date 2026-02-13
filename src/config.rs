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
    #[serde(default = "default_socket_timeout")]
    pub socket_timeout_secs: u64,
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

fn default_socket_timeout() -> u64 {
    30
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "json".to_string()
}

impl AgentConfig {
    pub fn from_file(path: &Path) -> Result<Self, AgentError> {
        let content = std::fs::read_to_string(path).map_err(|e| AgentError::io(path, e))?;
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
