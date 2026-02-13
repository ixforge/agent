use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

use crate::config::LoggingConfig;

/// Initialize tracing with stdout + optional file output
/// Returns guards that must be held for the lifetime of the program
pub fn init(config: &LoggingConfig) -> Vec<WorkerGuard> {
    let mut guards = Vec::new();

    let filter = EnvFilter::try_new(&config.level).unwrap_or_else(|_| EnvFilter::new("info"));

    let is_json = config.format == "json";

    if let Some(ref file_path) = config.file_path {
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
