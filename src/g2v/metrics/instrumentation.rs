//! Per-call timing utilities for service metrics.
//!
//! For full request-level instrumentation see
//! [`crate::g2v::middleware::InstrumentationLayer`]. This module provides
//! lower-level helpers for ad-hoc timing inside handler bodies.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::g2v::metrics::ServiceMetrics;

/// RAII guard that records request duration when dropped.
#[derive(Debug)]
pub struct TimingGuard {
    metrics: Arc<ServiceMetrics>,
    service_name: String,
    method: String,
    start: Instant,
    armed: bool,
}

impl TimingGuard {
    /// Create a new timing guard that will record duration to `metrics` on drop.
    pub fn new(
        metrics: Arc<ServiceMetrics>,
        service_name: impl Into<String>,
        method: impl Into<String>,
    ) -> Self {
        Self {
            metrics,
            service_name: service_name.into(),
            method: method.into(),
            start: Instant::now(),
            armed: true,
        }
    }

    /// Return the elapsed time since the guard was created.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Record duration immediately and disarm the drop hook.
    pub fn record(mut self) -> Duration {
        let duration = self.elapsed();
        self.metrics
            .request_duration
            .with_label_values(&[self.service_name.as_str(), self.method.as_str()])
            .observe(duration.as_secs_f64());
        self.armed = false;
        duration
    }
}

impl Drop for TimingGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let duration = self.elapsed();
        self.metrics
            .request_duration
            .with_label_values(&[self.service_name.as_str(), self.method.as_str()])
            .observe(duration.as_secs_f64());
    }
}

/// Start timing a method call. The returned guard records duration on drop.
pub fn start_timing(
    metrics: Arc<ServiceMetrics>,
    service_name: impl Into<String>,
    method: impl Into<String>,
) -> TimingGuard {
    TimingGuard::new(metrics, service_name, method)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_guard_elapsed() {
        let metrics = Arc::new(ServiceMetrics::new("test-service").unwrap());
        let guard = TimingGuard::new(metrics, "test-service", "test-method");
        std::thread::sleep(Duration::from_millis(10));
        assert!(guard.elapsed() >= Duration::from_millis(10));
    }

    #[test]
    fn test_timing_guard_record() {
        let metrics = Arc::new(ServiceMetrics::new("test-service").unwrap());
        let guard = TimingGuard::new(metrics, "test-service", "test-method");
        let duration = guard.record();
        assert!(duration >= Duration::ZERO);
    }

    #[test]
    fn test_start_timing() {
        let metrics = Arc::new(ServiceMetrics::new("test-service").unwrap());
        let guard = start_timing(metrics, "test-service", "test-method");
        assert!(guard.elapsed() >= Duration::ZERO);
    }
}
