use sysinfo::System;

pub struct SystemMetrics {
    system: System,
}

impl SystemMetrics {
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
        }
    }

    pub fn refresh(&mut self) {
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
    }

    pub fn cpu_usage(&self) -> f64 {
        self.system.global_cpu_usage() as f64
    }

    pub fn memory_usage_percent(&self) -> f64 {
        let total = self.system.total_memory() as f64;
        let used = self.system.used_memory() as f64;
        if total > 0.0 {
            (used / total) * 100.0
        } else {
            0.0
        }
    }
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self::new()
    }
}
