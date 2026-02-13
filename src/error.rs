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
