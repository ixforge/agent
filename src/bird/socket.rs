use std::path::PathBuf;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::{Duration, timeout};
use tracing::trace;

use super::BirdClient;
use crate::error::AgentError;

/// Tope por defecto de una respuesta del socket de BIRD
///
/// El dump de rutas del upstream pasa de 45 MB, asi que 16 MiB cortaba la
/// lectura. El tope sigue existiendo para que un BIRD que nunca mande el
/// marcador de fin no haga crecer la memoria sin limite
const MAX_RESPONSE_MB_POR_DEFECTO: usize = 128;

pub struct BirdSocketClient {
    socket_path: PathBuf,
    timeout: Duration,
    max_response_bytes: usize,
}

impl BirdSocketClient {
    pub fn new(socket_path: &str, timeout_secs: u64) -> Self {
        Self::with_max_response(socket_path, timeout_secs, MAX_RESPONSE_MB_POR_DEFECTO)
    }

    pub fn with_max_response(socket_path: &str, timeout_secs: u64, max_mb: usize) -> Self {
        Self {
            socket_path: PathBuf::from(socket_path),
            timeout: Duration::from_secs(timeout_secs),
            max_response_bytes: max_mb * 1024 * 1024,
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
            if response.len().saturating_add(n) > self.max_response_bytes {
                return Err(AgentError::BirdSocket(format!(
                    "response exceeded {} bytes without end marker",
                    self.max_response_bytes
                )));
            }
            response.push_str(&String::from_utf8_lossy(&buf[..n]));

            if Self::has_end_marker(&response) {
                break;
            }
        }

        Ok(response)
    }

    /// Check the last *complete* line of the accumulated response for a
    /// BIRD end marker. Completeness requires a trailing newline — otherwise
    /// we may be looking at a prefix of more data still in flight. An end
    /// marker is a 4-digit status code either alone on the line ("0000")
    /// or followed by a space and a human message ("0013 Daemon is up").
    /// Continuation lines use a dash ("1002-...") and must NOT terminate.
    fn has_end_marker(response: &str) -> bool {
        if !response.ends_with('\n') {
            return false;
        }
        response.lines().next_back().is_some_and(|line| {
            let bytes = line.as_bytes();
            bytes.len() >= 4
                && bytes[..4].iter().all(u8::is_ascii_digit)
                && (bytes.len() == 4 || bytes[4] == b' ')
        })
    }
}

#[cfg(test)]
mod tests {
    use super::BirdSocketClient;

    fn end(s: &str) -> bool {
        BirdSocketClient::has_end_marker(s)
    }

    #[test]
    fn end_marker_with_message() {
        assert!(end("0013 Daemon is up\n"));
    }

    #[test]
    fn end_marker_alone() {
        assert!(end("0000\n"));
    }

    #[test]
    fn continuation_is_not_end() {
        assert!(!end("1002-protocol bird_bgp\n"));
    }

    #[test]
    fn partial_line_without_newline_is_not_end() {
        // We might be mid-receive; do not terminate until newline arrives.
        assert!(!end("0000"));
        assert!(!end("0013 Daemon is up"));
    }

    #[test]
    fn end_marker_after_prior_lines() {
        let resp = "1002-protocol bird_bgp\n1002 peer 1.2.3.4\n0000 \n";
        assert!(end(resp));
    }

    #[test]
    fn non_digit_prefix_is_not_end() {
        assert!(!end("INFO something happened\n"));
    }
}
