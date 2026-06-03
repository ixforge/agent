use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::info;

use super::BirdClient;
use crate::bird::parser::{BirdProtocol, parse_bird_uptime, parse_protocols};
use crate::error::AgentError;

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
            .map_err(|e| {
                AgentError::BirdValidation(format!(
                    "failed to run {}: {e}",
                    self.bird_binary.display()
                ))
            })?;

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
        Ok(parse_protocols(&output))
    }

    /// Get BIRD uptime in seconds by parsing `show status` output
    pub async fn get_uptime(&self) -> Option<f64> {
        let output = self.client.send_command("show status").await.ok()?;
        parse_bird_uptime(&output)
    }

    /// Check if BIRD is running
    pub async fn is_running(&self) -> bool {
        self.client.is_running().await
    }

    /// Atomically install a previously-validated config file at `temp_path`
    /// by renaming it over `self.config_path` and fsyncing the parent dir.
    /// Rename is atomic within a single filesystem, so a crash cannot leave
    /// the live config partially written.
    pub async fn commit_config(&self, temp_path: &Path) -> Result<(), AgentError> {
        tokio::fs::rename(temp_path, &self.config_path)
            .await
            .map_err(|e| AgentError::io(&self.config_path, e))?;

        if let Some(parent) = self.config_path.parent() {
            let dir = tokio::fs::File::open(parent)
                .await
                .map_err(|e| AgentError::io(parent, e))?;
            dir.sync_all()
                .await
                .map_err(|e| AgentError::io(parent, e))?;
        }
        Ok(())
    }
}
