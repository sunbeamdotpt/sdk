//! Service-level metrics for Sunbeam services.

use std::sync::Arc;

use prometheus::{CounterVec, GaugeVec, HistogramVec};

use crate::g2v::metrics::registry::MetricsRegistry;

/// Service metrics container.
///
/// This holds all service-level metrics in one place for easy access.
/// All metrics are automatically labeled with service name and method name.
#[derive(Debug, Clone)]
pub struct ServiceMetrics {
    /// The metrics registry.
    registry: Arc<MetricsRegistry>,
    /// Service name.
    service_name: String,
    /// Total request counter (by service, method, status).
    pub requests_total: CounterVec,
    /// Request duration histogram (by service, method).
    pub request_duration: HistogramVec,
    /// Active requests gauge (by service, method).
    pub active_requests: GaugeVec,
    /// Success counter (by service, method).
    pub success_count: CounterVec,
    /// Error counter (by service, method, error_code).
    pub error_count: CounterVec,
    /// Errors total counter (by method, path, error_type) - legacy.
    pub errors_total: CounterVec,
}

impl ServiceMetrics {
    /// Create new service metrics.
    ///
    /// # Errors
    ///
    /// Returns the Prometheus registration error if any built-in metric fails
    /// to register (e.g. an invalid definition).
    pub fn new(service_name: impl Into<String>) -> Result<Self, prometheus::Error> {
        let service_name_str = service_name.into();
        let registry = Arc::new(MetricsRegistry::new(&service_name_str));

        let requests_total = registry.register_counter_vec(
            "requests_total",
            "Total number of requests",
            &["service", "method"],
        )?;

        let request_duration = registry.register_histogram_vec(
            "request_duration_seconds",
            "Request duration in seconds",
            None,
            &["service", "method"],
        )?;

        let active_requests = registry.register_gauge_vec(
            "active_requests",
            "Number of currently active requests",
            &["service", "method"],
        )?;

        let success_count = registry.register_counter_vec(
            "success_count",
            "Total number of successful requests",
            &["service", "method"],
        )?;

        let error_count = registry.register_counter_vec(
            "error_count",
            "Total number of errors",
            &["service", "method", "error_code"],
        )?;

        let errors_total = registry.register_counter_vec(
            "errors_total",
            "Total number of errors (legacy)",
            &["method", "path", "error_type"],
        )?;

        Ok(Self {
            registry,
            service_name: service_name_str,
            requests_total,
            request_duration,
            active_requests,
            success_count,
            error_count,
            errors_total,
        })
    }

    /// Get the underlying registry.
    pub fn registry(&self) -> &Arc<MetricsRegistry> {
        &self.registry
    }

    /// Get the service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Increment the request counter.
    pub fn increment_requests(&self, method: &str, _path: &str, _status: &str) {
        self.requests_total
            .with_label_values(&[self.service_name.as_str(), method])
            .inc();
    }

    /// Record request duration.
    pub fn record_duration(&self, method: &str, _path: &str, duration: std::time::Duration) {
        self.request_duration
            .with_label_values(&[self.service_name.as_str(), method])
            .observe(duration.as_secs_f64());
    }

    /// Increment active requests.
    pub fn increment_active(&self, method: &str) {
        self.active_requests
            .with_label_values(&[self.service_name.as_str(), method])
            .inc();
    }

    /// Decrement active requests.
    pub fn decrement_active(&self, method: &str) {
        self.active_requests
            .with_label_values(&[self.service_name.as_str(), method])
            .dec();
    }

    /// Increment error counter.
    pub fn increment_errors(&self, method: &str, path: &str, error_type: &str) {
        self.errors_total
            .with_label_values(&[method, path, error_type])
            .inc();
    }

    /// Gather and encode all metrics.
    pub async fn gather_and_encode(&self) -> Result<String, prometheus::Error> {
        self.registry.gather_and_encode().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_metrics_new() {
        let metrics = ServiceMetrics::new("test-service").unwrap();
        assert_eq!(metrics.service_name(), "test-service");
    }

    #[test]
    fn test_service_metrics_default() {
        let metrics = ServiceMetrics::new("sunbeam-g2v").unwrap();
        assert_eq!(metrics.service_name(), "sunbeam-g2v");
    }

    #[test]
    fn test_service_metrics_increment_requests() {
        let metrics = ServiceMetrics::new("test-service").unwrap();
        metrics.increment_requests("GET", "/health", "200");
    }

    #[tokio::test]
    async fn test_service_metrics_gather() {
        let metrics = ServiceMetrics::new("test-service").unwrap();
        // Increment a metric so it gets registered and appears in the output
        metrics.increment_requests("GET", "/test", "200");
        let encoded = metrics.gather_and_encode().await.unwrap();
        // The metric should be in the output
        assert!(!encoded.is_empty());
    }

    #[tokio::test]
    async fn test_service_metrics_all_helpers() {
        let metrics = ServiceMetrics::new("coverage-service").unwrap();
        assert_eq!(metrics.service_name(), "coverage-service");
        assert_eq!(metrics.registry().service_name(), "coverage-service");

        metrics.increment_requests("Get", "/ignored", "200");
        metrics.record_duration("Get", "/ignored", std::time::Duration::from_millis(25));
        metrics.increment_active("Get");
        metrics.decrement_active("Get");
        metrics
            .success_count
            .with_label_values(&["coverage-service", "Get"])
            .inc();
        metrics
            .error_count
            .with_label_values(&["coverage-service", "Get", "Internal"])
            .inc();
        metrics.increment_errors("Get", "/rpc", "Internal");

        let encoded = metrics.gather_and_encode().await.unwrap();
        for expected in [
            "requests_total",
            "request_duration_seconds",
            "active_requests",
            "success_count",
            "error_count",
            "errors_total",
            "coverage-service",
            "Internal",
        ] {
            assert!(encoded.contains(expected), "missing {expected}");
        }
    }
}
