//! Instrumentation middleware for automatic metrics and tracing.
//!
//! This module provides a Tower layer that automatically instruments all RPC requests
//! with metrics (counters, histograms, gauges) and distributed tracing spans.

use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context as TaskContext, Poll},
    time::Instant,
};

use http::Request;
use tower::{Layer, Service};

use crate::g2v::metrics::ServiceMetrics;

/// Instrumentation layer that wraps services with automatic metrics and tracing.
///
/// This layer intercepts all requests and:
/// - Records request counts by service/method
/// - Records request durations
/// - Tracks active requests
/// - Creates tracing spans with service/method context
/// - Records success/error counts
#[derive(Debug, Clone)]
pub struct InstrumentationLayer {
    /// The service metrics registry.
    metrics: Arc<ServiceMetrics>,
    /// The service name for labeling.
    service_name: String,
}

impl InstrumentationLayer {
    /// Create a new instrumentation layer.
    pub fn new(metrics: Arc<ServiceMetrics>, service_name: impl Into<String>) -> Self {
        Self {
            metrics,
            service_name: service_name.into(),
        }
    }
}

impl<S> Layer<S> for InstrumentationLayer {
    type Service = InstrumentationService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        InstrumentationService {
            inner,
            metrics: Arc::clone(&self.metrics),
            service_name: self.service_name.clone(),
        }
    }
}

/// The instrumentation service wrapper.
#[derive(Debug, Clone)]
pub struct InstrumentationService<S> {
    inner: S,
    metrics: Arc<ServiceMetrics>,
    service_name: String,
}

impl<S, B> Service<Request<B>> for InstrumentationService<S>
where
    S: Service<Request<B>> + Clone + 'static,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = InstrumentationFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let metrics = Arc::clone(&self.metrics);
        let service_name = self.service_name.clone();

        // Extract method from the request path
        // ConnectRPC paths are typically: /package.ServiceName/MethodName
        let path = req.uri().path();
        let (rpc_service, method) = parse_rpc_path(path);
        let full_method = format!("{}.{}", rpc_service, method);

        // Increment active requests
        metrics
            .active_requests
            .with_label_values(&[&service_name, &full_method])
            .inc();

        // Start timing
        let start = Instant::now();

        // Create tracing span with dynamic field values
        // tracing 0.1 requires level parameter
        let span = tracing::span!(
            target: "sdk::g2v",
            tracing::Level::INFO,
            "rpc",
        );
        span.record("service", &service_name);
        span.record("rpc_service", &rpc_service);
        span.record("method", &method);

        // Call inner service
        let inner_fut = self.inner.clone().call(req);

        InstrumentationFuture {
            inner_fut: Box::pin(inner_fut),
            metrics,
            service_name,
            rpc_service,
            method,
            start,
            span,
        }
    }
}

/// Future for the instrumentation service.
pub struct InstrumentationFuture<F> {
    inner_fut: Pin<Box<F>>,
    metrics: Arc<ServiceMetrics>,
    service_name: String,
    rpc_service: String,
    method: String,
    start: Instant,
    span: tracing::Span,
}

impl<F, T, E> Future for InstrumentationFuture<F>
where
    F: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    type Output = Result<T, E>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        match this.inner_fut.as_mut().poll(cx) {
            Poll::Ready(result) => {
                let duration = this.start.elapsed();
                let full_method = format!("{}.{}", this.rpc_service, this.method);

                // Decrement active requests
                this.metrics
                    .active_requests
                    .with_label_values(&[&this.service_name, &full_method])
                    .dec();

                // Record duration
                this.metrics
                    .request_duration
                    .with_label_values(&[&this.service_name, &full_method])
                    .observe(duration.as_secs_f64());

                // Record count
                this.metrics
                    .requests_total
                    .with_label_values(&[&this.service_name, &full_method])
                    .inc();

                // Record result
                match &result {
                    Ok(_) => {
                        this.metrics
                            .success_count
                            .with_label_values(&[&this.service_name, &full_method])
                            .inc();
                        this.span.record("status", "ok");
                    }
                    Err(e) => {
                        let error_str = e.to_string();
                        let error_code = extract_error_code(&error_str);

                        this.metrics
                            .error_count
                            .with_label_values(&[&this.service_name, &full_method, error_code])
                            .inc();
                        this.span.record("status", "error");
                        this.span.record("error", error_str);
                    }
                }

                this.span.record("duration_secs", duration.as_secs_f64());

                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Parse an RPC path to extract service and method names.
///
/// ConnectRPC paths are typically: /package.ServiceName/MethodName
fn parse_rpc_path(path: &str) -> (String, String) {
    // Remove leading slash
    let trimmed = path.trim_start_matches('/');

    // Split on the last '/' to get service and method
    if let Some(pos) = trimmed.rfind('/') {
        let service = &trimmed[..pos];
        let method = &trimmed[pos + 1..];
        (service.to_string(), method.to_string())
    } else {
        // No slash found - use full path as service, "unknown" as method
        (trimmed.to_string(), "unknown".to_string())
    }
}

/// Extract error code from error string representation.
fn extract_error_code(error_str: &str) -> &'static str {
    if error_str.contains("InvalidArgument") {
        "InvalidArgument"
    } else if error_str.contains("NotFound") {
        "NotFound"
    } else if error_str.contains("AlreadyExists") {
        "AlreadyExists"
    } else if error_str.contains("PermissionDenied") {
        "PermissionDenied"
    } else if error_str.contains("Unauthenticated") {
        "Unauthenticated"
    } else if error_str.contains("Internal") {
        "Internal"
    } else if error_str.contains("Unavailable") {
        "Unavailable"
    } else if error_str.contains("Unimplemented") {
        "Unimplemented"
    } else if error_str.contains("DeadlineExceeded") {
        "DeadlineExceeded"
    } else {
        "Unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rpc_path() {
        assert_eq!(
            parse_rpc_path("/mypackage.MyService/MyMethod"),
            ("mypackage.MyService".to_string(), "MyMethod".to_string())
        );
        assert_eq!(
            parse_rpc_path("/Service/Method"),
            ("Service".to_string(), "Method".to_string())
        );
        assert_eq!(
            parse_rpc_path("/Single"),
            ("Single".to_string(), "unknown".to_string())
        );
    }

    #[test]
    fn test_extract_error_code() {
        assert_eq!(
            extract_error_code("InvalidArgument: bad input"),
            "InvalidArgument"
        );
        assert_eq!(extract_error_code("NotFound: resource missing"), "NotFound");
        assert_eq!(extract_error_code("Internal: server error"), "Internal");
        assert_eq!(extract_error_code("Unknown error"), "Unknown");
    }

    #[test]
    fn test_parse_rpc_path_edge_cases() {
        assert_eq!(parse_rpc_path(""), ("".to_string(), "unknown".to_string()));
        assert_eq!(parse_rpc_path("/"), ("".to_string(), "unknown".to_string()));
        assert_eq!(
            parse_rpc_path("svc/method"),
            ("svc".to_string(), "method".to_string())
        );
        assert_eq!(
            parse_rpc_path("/pkg.Service/"),
            ("pkg.Service".to_string(), "".to_string())
        );
    }

    #[test]
    fn test_extract_error_code_all_known_codes() {
        for code in [
            "InvalidArgument",
            "NotFound",
            "AlreadyExists",
            "PermissionDenied",
            "Unauthenticated",
            "Internal",
            "Unavailable",
            "Unimplemented",
            "DeadlineExceeded",
        ] {
            assert_eq!(extract_error_code(&format!("prefix {code} suffix")), code);
        }
        assert_eq!(extract_error_code("something else"), "Unknown");
    }

    #[tokio::test]
    async fn test_instrumentation_records_success() {
        use bytes::Bytes;
        use http::Response;
        use tower::{ServiceExt as _, service_fn};

        let metrics = Arc::new(ServiceMetrics::new("coverage-success").unwrap());
        let layer = InstrumentationLayer::new(Arc::clone(&metrics), "coverage-success");
        let mut service = layer.layer(service_fn(|_req: Request<Bytes>| async {
            Ok::<_, std::convert::Infallible>(Response::new(Bytes::from_static(b"ok")))
        }));

        service.ready().await.unwrap();
        let response = service.call(Request::new(Bytes::new())).await.unwrap();
        assert_eq!(response.body().as_ref(), b"ok");

        let encoded = metrics.gather_and_encode().await.unwrap();
        assert!(encoded.contains("coverage-success"));
        assert!(encoded.contains("unknown"));
        assert!(encoded.contains("success_count"));
        assert!(encoded.contains("requests_total"));
        assert!(encoded.contains("request_duration_seconds"));
    }

    #[derive(Debug)]
    struct TestError(&'static str);

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }

    #[tokio::test]
    async fn test_instrumentation_records_error_code() {
        use bytes::Bytes;
        use http::Response;
        use tower::{ServiceExt as _, service_fn};

        let metrics = Arc::new(ServiceMetrics::new("coverage-error").unwrap());
        let layer = InstrumentationLayer::new(Arc::clone(&metrics), "coverage-error");
        let mut service = layer.layer(service_fn(|_req: Request<Bytes>| async {
            Err::<Response<Bytes>, _>(TestError("PermissionDenied: no access"))
        }));

        service.ready().await.unwrap();
        let error = service
            .call(
                Request::builder()
                    .uri("/pkg.Secrets/Get")
                    .body(Bytes::new())
                    .unwrap(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "PermissionDenied: no access");

        let encoded = metrics.gather_and_encode().await.unwrap();
        assert!(encoded.contains("pkg.Secrets.Get"));
        assert!(encoded.contains("PermissionDenied"));
        assert!(encoded.contains("error_count"));
    }
}
