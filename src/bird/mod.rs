pub mod manager;
pub mod parser;
pub mod socket;

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
