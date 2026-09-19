//! Prometheus metrics registry for Sunbeam services.

use std::sync::Arc;

use prometheus::{HistogramOpts, Opts, Registry};

/// Metrics registry wrapper.
///
/// This wraps Prometheus's Registry with Sunbeam-specific functionality.
#[derive(Debug, Clone)]
pub struct MetricsRegistry {
    /// The underlying Prometheus registry.
    registry: Arc<Registry>,
    /// Service name for default labels.
    service_name: String,
}

impl MetricsRegistry {
    /// Create a new metrics registry.
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            registry: Arc::new(Registry::new()),
            service_name: service_name.into(),
        }
    }

    /// Get a reference to the underlying Prometheus registry.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Get the service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Register a counter metric without labels.
    pub fn register_counter(
        &self,
        name: &str,
        help: &str,
    ) -> Result<prometheus::Counter, prometheus::Error> {
        let counter = prometheus::Counter::with_opts(Opts::new(name, help))?;
        self.registry.register(Box::new(counter.clone()))?;
        Ok(counter)
    }

    /// Register a counter metric with labels.
    pub fn register_counter_vec(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
    ) -> Result<prometheus::CounterVec, prometheus::Error> {
        let counter_vec = prometheus::CounterVec::new(Opts::new(name, help), labels)?;
        self.registry.register(Box::new(counter_vec.clone()))?;
        Ok(counter_vec)
    }

    /// Register a gauge metric without labels.
    pub fn register_gauge(
        &self,
        name: &str,
        help: &str,
    ) -> Result<prometheus::Gauge, prometheus::Error> {
        let gauge = prometheus::Gauge::with_opts(Opts::new(name, help))?;
        self.registry.register(Box::new(gauge.clone()))?;
        Ok(gauge)
    }

    /// Register a gauge metric with labels.
    pub fn register_gauge_vec(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
    ) -> Result<prometheus::GaugeVec, prometheus::Error> {
        let gauge_vec = prometheus::GaugeVec::new(Opts::new(name, help), labels)?;
        self.registry.register(Box::new(gauge_vec.clone()))?;
        Ok(gauge_vec)
    }

    /// Register a histogram metric without labels.
    pub fn register_histogram(
        &self,
        name: &str,
        help: &str,
        buckets: Option<&[f64]>,
    ) -> Result<prometheus::Histogram, prometheus::Error> {
        let mut opts = HistogramOpts::new(name, help);
        if let Some(buckets) = buckets {
            opts = opts.buckets(buckets.to_vec());
        }
        let histogram = prometheus::Histogram::with_opts(opts)?;
        self.registry.register(Box::new(histogram.clone()))?;
        Ok(histogram)
    }

    /// Register a histogram metric with labels.
    pub fn register_histogram_vec(
        &self,
        name: &str,
        help: &str,
        buckets: Option<&[f64]>,
        labels: &[&str],
    ) -> Result<prometheus::HistogramVec, prometheus::Error> {
        let mut opts = HistogramOpts::new(name, help);
        if let Some(buckets) = buckets {
            opts = opts.buckets(buckets.to_vec());
        }
        let histogram_vec = prometheus::HistogramVec::new(opts, labels)?;
        self.registry.register(Box::new(histogram_vec.clone()))?;
        Ok(histogram_vec)
    }

    /// Gather all metrics and encode as text.
    pub async fn gather_and_encode(&self) -> Result<String, prometheus::Error> {
        let encoder = prometheus::TextEncoder::new();
        let metrics = self.registry.gather();
        encoder.encode_to_string(&metrics)
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new("sunbeam-g2v")
    }
}

/// Create a metrics registry with default service metrics.
pub fn create_default_registry(service_name: impl Into<String>) -> MetricsRegistry {
    let registry = MetricsRegistry::new(service_name);

    // Register default metrics
    let _ = registry.register_counter_vec(
        "requests_total",
        "Total number of requests",
        &["method", "path", "status"],
    );
    let _ = registry.register_histogram_vec(
        "request_duration_seconds",
        "Request duration in seconds",
        None,
        &["method", "path"],
    );
    let _ = registry.register_gauge("active_requests", "Number of active requests");

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registry_new() {
        let registry = MetricsRegistry::new("test-service");
        assert_eq!(registry.service_name(), "test-service");
    }

    #[test]
    fn test_metrics_registry_default() {
        let registry = MetricsRegistry::default();
        assert_eq!(registry.service_name(), "sunbeam-g2v");
    }

    #[test]
    fn test_metrics_registry_register_counter() {
        let registry = MetricsRegistry::new("test-service");
        let counter = registry.register_counter("test_counter", "A test counter");
        assert!(counter.is_ok());
    }

    #[test]
    fn test_metrics_registry_register_counter_vec() {
        let registry = MetricsRegistry::new("test-service");
        let counter_vec =
            registry.register_counter_vec("test_counter_vec", "A test counter vec", &["label1"]);
        assert!(counter_vec.is_ok());
    }

    #[tokio::test]
    async fn test_metrics_registry_gather() {
        let registry = MetricsRegistry::new("test-service");
        let _ = registry
            .register_counter("test_counter", "A test counter")
            .unwrap();

        let encoded = registry.gather_and_encode().await.unwrap();
        assert!(encoded.contains("test_counter"));
    }

    #[test]
    fn test_metrics_registry_accessors_and_all_metric_types() {
        let registry = MetricsRegistry::new("coverage-service");
        assert_eq!(registry.service_name(), "coverage-service");
        assert!(registry.registry().gather().is_empty());

        let counter = registry
            .register_counter("coverage_counter", "counter")
            .unwrap();
        counter.inc();
        let counter_vec = registry
            .register_counter_vec("coverage_counter_vec", "counter vec", &["kind"])
            .unwrap();
        counter_vec.with_label_values(&["a"]).inc();
        let gauge = registry.register_gauge("coverage_gauge", "gauge").unwrap();
        gauge.set(3.0);
        let gauge_vec = registry
            .register_gauge_vec("coverage_gauge_vec", "gauge vec", &["kind"])
            .unwrap();
        gauge_vec.with_label_values(&["a"]).set(4.0);
        let histogram = registry
            .register_histogram("coverage_histogram", "histogram", Some(&[0.1, 1.0]))
            .unwrap();
        histogram.observe(0.5);
        let histogram_vec = registry
            .register_histogram_vec(
                "coverage_histogram_vec",
                "histogram vec",
                Some(&[0.1, 1.0]),
                &["kind"],
            )
            .unwrap();
        histogram_vec.with_label_values(&["a"]).observe(0.25);
        let default_histogram = registry
            .register_histogram("coverage_default_histogram", "default buckets", None)
            .unwrap();
        default_histogram.observe(0.5);
        let default_histogram_vec = registry
            .register_histogram_vec(
                "coverage_default_histogram_vec",
                "default buckets vec",
                None,
                &["kind"],
            )
            .unwrap();
        default_histogram_vec.with_label_values(&["a"]).observe(0.5);

        assert_eq!(registry.registry().gather().len(), 8);
    }

    #[test]
    fn test_metrics_registry_duplicate_and_invalid_metrics_fail() {
        let registry = MetricsRegistry::new("coverage-service");
        registry
            .register_counter("duplicate_counter", "counter")
            .unwrap();
        assert!(
            registry
                .register_counter("duplicate_counter", "counter again")
                .is_err()
        );
        assert!(
            registry
                .register_counter("not a metric", "invalid")
                .is_err()
        );
        assert!(
            registry
                .register_counter_vec("bad_counter_vec", "invalid", &["not a label"])
                .is_err()
        );
        assert!(
            registry
                .register_histogram("bad_histogram", "invalid", Some(&[1.0, 0.5]))
                .is_err()
        );
    }

    #[test]
    fn test_create_default_registry_uses_service_name() {
        let registry = create_default_registry("default-service");
        assert_eq!(registry.service_name(), "default-service");
    }
}
