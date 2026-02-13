use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use serde_json::json;
use tokio::net::TcpListener;

use super::registry::MetricsRegistry;

#[derive(Clone)]
struct AppState {
    metrics: Arc<MetricsRegistry>,
    core_connected: Arc<std::sync::atomic::AtomicBool>,
    bird_running: Arc<std::sync::atomic::AtomicBool>,
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.metrics.refresh();

    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": state.metrics.agent_uptime.get(),
        "bird": {
            "running": state.bird_running.load(std::sync::atomic::Ordering::Relaxed),
        },
        "core_connected": state.core_connected.load(std::sync::atomic::Ordering::Relaxed),
    }))
}

async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let body = state.metrics.encode();
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(state)
}

/// Run the metrics server with a pre-bound listener (for testing)
pub async fn run_with_listener(listener: TcpListener) {
    let state = AppState {
        metrics: MetricsRegistry::new(),
        core_connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        bird_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    let app = build_router(state);

    tracing::info!(
        addr = %listener.local_addr().unwrap(),
        "metrics server listening"
    );

    axum::serve(listener, app).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "metrics server error");
    });
}

/// Run the metrics server with shared state from the main loop
pub async fn run_with_state(
    listener: TcpListener,
    metrics: Arc<MetricsRegistry>,
    core_connected: Arc<std::sync::atomic::AtomicBool>,
    bird_running: Arc<std::sync::atomic::AtomicBool>,
) {
    let state = AppState {
        metrics,
        core_connected,
        bird_running,
    };

    let app = build_router(state);

    tracing::info!(
        addr = %listener.local_addr().unwrap(),
        "metrics server listening"
    );

    axum::serve(listener, app).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "metrics server error");
    });
}
