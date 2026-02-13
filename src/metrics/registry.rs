use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;

use super::system::SystemMetrics;

pub struct MetricsRegistry {
    registry: Registry,
    pub agent_uptime: Gauge,
    pub poll_errors_total: Counter,
    pub bgp_sessions_up: Gauge,
    pub bgp_sessions_total: Gauge,
    pub host_cpu_usage: Gauge<f64, AtomicU64>,
    pub host_memory_usage: Gauge<f64, AtomicU64>,
    started_at: Instant,
    system_metrics: Mutex<SystemMetrics>,
}

impl MetricsRegistry {
    pub fn new() -> Arc<Self> {
        let mut registry = Registry::default();

        let agent_uptime = Gauge::default();
        registry.register(
            "ixforge_agent_uptime_seconds",
            "Agent uptime in seconds",
            agent_uptime.clone(),
        );

        let poll_errors_total = Counter::default();
        registry.register(
            "ixforge_agent_poll_errors_total",
            "Total number of Core API poll errors",
            poll_errors_total.clone(),
        );

        let bgp_sessions_up = Gauge::default();
        registry.register(
            "ixforge_agent_bgp_sessions_up",
            "Number of BGP sessions in established state",
            bgp_sessions_up.clone(),
        );

        let bgp_sessions_total = Gauge::default();
        registry.register(
            "ixforge_agent_bgp_sessions_total",
            "Total number of BGP sessions",
            bgp_sessions_total.clone(),
        );

        let host_cpu_usage = Gauge::<f64, _>::default();
        registry.register(
            "ixforge_agent_host_cpu_usage_percent",
            "Host CPU usage percentage",
            host_cpu_usage.clone(),
        );

        let host_memory_usage = Gauge::<f64, _>::default();
        registry.register(
            "ixforge_agent_host_memory_usage_percent",
            "Host memory usage percentage",
            host_memory_usage.clone(),
        );

        Arc::new(Self {
            registry,
            agent_uptime,
            poll_errors_total,
            bgp_sessions_up,
            bgp_sessions_total,
            host_cpu_usage,
            host_memory_usage,
            started_at: Instant::now(),
            system_metrics: Mutex::new(SystemMetrics::new()),
        })
    }

    /// Update dynamic metrics before encoding
    pub fn refresh(&self) {
        self.agent_uptime
            .set(self.started_at.elapsed().as_secs() as i64);

        if let Ok(mut sys) = self.system_metrics.lock() {
            sys.refresh();
            self.host_cpu_usage.set(sys.cpu_usage());
            self.host_memory_usage.set(sys.memory_usage_percent());
        }
    }

    pub fn encode(&self) -> String {
        self.refresh();
        let mut buf = String::new();
        encode(&mut buf, &self.registry).unwrap_or_default();
        buf
    }
}
