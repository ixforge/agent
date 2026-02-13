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
