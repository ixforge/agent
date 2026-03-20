use std::path::PathBuf;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::{Duration, timeout};
use tracing::trace;

use super::BirdClient;
use crate::error::AgentError;

pub struct BirdSocketClient {
    socket_path: PathBuf,
    timeout: Duration,
}

impl BirdSocketClient {
    pub fn new(socket_path: &str, timeout_secs: u64) -> Self {
        Self {
            socket_path: PathBuf::from(socket_path),
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

impl BirdClient for BirdSocketClient {
    async fn send_command(&self, command: &str) -> Result<String, AgentError> {
        timeout(self.timeout, self.send_command_inner(command))
            .await
            .map_err(|_| {
                AgentError::BirdSocket(format!(
                    "socket operation timed out after {}s",
                    self.timeout.as_secs()
                ))
            })?
    }

    async fn is_running(&self) -> bool {
        let probe_timeout = self.timeout.min(Duration::from_secs(5));
        timeout(probe_timeout, UnixStream::connect(&self.socket_path))
            .await
            .is_ok_and(|r| r.is_ok())
    }
}

impl BirdSocketClient {
    async fn send_command_inner(&self, command: &str) -> Result<String, AgentError> {
        let mut stream = UnixStream::connect(&self.socket_path).await.map_err(|e| {
            AgentError::BirdSocket(format!(
                "failed to connect to {}: {e}",
                self.socket_path.display()
            ))
        })?;

        // Read and discard the welcome banner
        let mut banner = vec![0u8; 4096];
        let n = stream
            .read(&mut banner)
            .await
            .map_err(|e| AgentError::BirdSocket(format!("failed to read banner: {e}")))?;
        trace!(banner = %String::from_utf8_lossy(&banner[..n]), "BIRD banner");

        // Send the command
        let cmd = format!("{command}\n");
        stream
            .write_all(cmd.as_bytes())
            .await
            .map_err(|e| AgentError::BirdSocket(format!("failed to send command: {e}")))?;

        // Read the full response until we see a BIRD end-of-response marker
        // (4-digit code followed by space, not '-')
        let mut response = String::new();
        let mut buf = vec![0u8; 8192];
        loop {
            let n = stream
                .read(&mut buf)
                .await
                .map_err(|e| AgentError::BirdSocket(format!("failed to read response: {e}")))?;
            if n == 0 {
                break;
            }
            response.push_str(&String::from_utf8_lossy(&buf[..n]));

            if Self::has_end_marker(&response) {
                break;
            }
        }

        Ok(response)
    }

    /// Check the last line of the accumulated response for a BIRD end marker.
    /// A line like "0000 " or "0013 Daemon is up" signals end of response.
    fn has_end_marker(response: &str) -> bool {
        response.lines().last().is_some_and(|line| {
            line.len() >= 5
                && line.as_bytes()[4] == b' '
                && line[..4].chars().all(|c| c.is_ascii_digit())
        })
    }
}
