use std::path::PathBuf;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::{Duration, timeout};

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
        timeout(
            Duration::from_secs(5),
            UnixStream::connect(&self.socket_path),
        )
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

        // Read the welcome banner
        let mut banner = vec![0u8; 4096];
        let n = stream
            .read(&mut banner)
            .await
            .map_err(|e| AgentError::BirdSocket(format!("failed to read banner: {e}")))?;
        let _banner_str = String::from_utf8_lossy(&banner[..n]);

        // Send the command
        let cmd = format!("{command}\n");
        stream
            .write_all(cmd.as_bytes())
            .await
            .map_err(|e| AgentError::BirdSocket(format!("failed to send command: {e}")))?;

        // Read the full response
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

            // BIRD protocol: lines starting with a 4-digit code and space (not '-') indicate end
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
}
