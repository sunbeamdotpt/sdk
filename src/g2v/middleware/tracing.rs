//! OpenTelemetry tracing middleware.
//!
//! Creates a server span for every request, extracts the W3C `traceparent` /
//! `tracestate` context from incoming headers so distributed traces connect
//! across services, and records the request id (populated by
//! [`RequestIdLayer`](crate::g2v::middleware::request_id::RequestIdLayer)) and the
//! response status onto the span.
//!
//! Applied by default to every [`AxumServer`](crate::g2v::server::axum::AxumServer)
//! route when the `tracing` feature is enabled. Spans only reach a collector
//! when [`crate::g2v::telemetry::init`] (or an equivalent provider setup) has run.

use tower::{Layer, Service};
use tracing::Instrument as _;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

/// `tracing` field carrying the OTel span name override.
const OTEL_NAME: &str = "otel.name";

/// Extractor adapter over request headers for the OTel propagator.
struct HeaderExtractor<'a>(&'a http::HeaderMap);

impl opentelemetry::propagation::Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

/// Split a ConnectRPC path (`/pkg.Service/Method`) into (service, method).
fn parse_connect_path(path: &str) -> Option<(&str, &str)> {
    let trimmed = path.strip_prefix('/')?;
    let (service, method) = trimmed.split_once('/')?;
    if service.is_empty() || method.is_empty() || method.contains('/') {
        return None;
    }
    Some((service, method))
}

/// Tracing middleware layer.
#[derive(Debug, Clone, Copy, Default)]
pub struct TracingLayer;

impl<S> Layer<S> for TracingLayer {
    type Service = TracingService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TracingService { inner }
    }
}

/// Tracing middleware service.
#[derive(Debug, Clone)]
pub struct TracingService<S> {
    inner: S,
}

impl<S, B, ResB> Service<http::Request<B>> for TracingService<S>
where
    S: Service<http::Request<B>, Response = http::Response<ResB>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: std::fmt::Debug + Send + 'static,
    B: Send + 'static,
    ResB: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        let parent = opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&HeaderExtractor(req.headers()))
        });

        let method = req.method().clone();
        let path = req.uri().path().to_string();
        let connect = parse_connect_path(&path);

        let span_name = match connect {
            Some((service, method)) => format!("{service}/{method}"),
            None => format!("{method} {path}"),
        };

        let span = tracing::info_span!(
            "request",
            { OTEL_NAME } = span_name.as_str(),
            "otel.kind" = "server",
            "otel.status_code" = tracing::field::Empty,
            "rpc.system" = connect.map(|_| "connectrpc"),
            "rpc.service" = connect.map(|(service, _)| service),
            "rpc.method" = connect.map(|(_, method)| method),
            "http.request.method" = %method,
            "url.path" = path.as_str(),
            "request_id" = tracing::field::Empty,
            "http.response.status_code" = tracing::field::Empty,
        );
        if let Err(e) = span.set_parent(parent) {
            tracing::debug!(msg = "failed to attach extracted trace parent", error = %e);
        }

        let fut = self.inner.call(req);
        Box::pin(async move {
            let handle = span.clone();
            let result = fut.instrument(span).await;
            match &result {
                Ok(response) => {
                    let status = response.status();
                    handle.record("http.response.status_code", status.as_u16() as i64);
                    if status.is_server_error() {
                        handle.record("otel.status_code", "ERROR");
                    }
                }
                Err(e) => {
                    handle.record("otel.status_code", "ERROR");
                    handle.record("exception.message", format!("{e:?}").as_str());
                }
            }
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_layer_creation() {
        let _ = TracingLayer;
    }

    #[test]
    fn test_parse_connect_path() {
        assert_eq!(
            parse_connect_path("/connectrpc.eliza.v1.ElizaService/Say"),
            Some(("connectrpc.eliza.v1.ElizaService", "Say"))
        );
        assert_eq!(parse_connect_path("/health/live"), Some(("health", "live")));
        assert_eq!(parse_connect_path("/"), None);
        assert_eq!(parse_connect_path("/a/b/c"), None);
        assert_eq!(parse_connect_path("no-leading-slash"), None);
    }

    #[tokio::test]
    async fn test_creates_server_span_and_records_status() {
        use opentelemetry_sdk::trace::{SdkTracerProvider, SimpleSpanProcessor};
        use tracing_subscriber::layer::SubscriberExt as _;

        // The middleware extracts context via the global propagator.
        opentelemetry::global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );

        // In-memory exporter to capture the finished span.
        let captured: std::sync::Arc<std::sync::Mutex<Vec<opentelemetry_sdk::trace::SpanData>>> =
            Default::default();
        let exporter = InMemoryExporter {
            spans: captured.clone(),
        };
        let provider = SdkTracerProvider::builder()
            .with_span_processor(SimpleSpanProcessor::new(exporter))
            .build();
        let tracer = opentelemetry::trace::TracerProvider::tracer(&provider, "test");
        let layer = tracing_opentelemetry::layer().with_tracer(tracer);
        let subscriber = tracing_subscriber::registry().with(layer);

        let service = tower::service_fn(|_req: http::Request<()>| {
            std::future::ready(Ok::<_, std::convert::Infallible>(
                http::Response::builder()
                    .status(http::StatusCode::OK)
                    .body(())
                    .expect("valid response"),
            ))
        });

        let guard = tracing::subscriber::set_default(subscriber);
        let mut service = TracingLayer.layer(service);
        let mut request = http::Request::new(());
        *request.uri_mut() = "/connectrpc.eliza.v1.ElizaService/Say"
            .parse()
            .expect("uri");
        request.headers_mut().insert(
            "traceparent",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
                .parse()
                .expect("header value"),
        );
        let response = Service::call(&mut service, request).await;
        assert!(response.is_ok());
        drop(guard);

        let spans = captured.lock().expect("lock").clone();
        provider.shutdown().expect("shutdown");
        assert_eq!(spans.len(), 1, "exactly one span exported");
        let span = &spans[0];
        assert_eq!(span.name, "connectrpc.eliza.v1.ElizaService/Say");
        assert_eq!(span.span_kind, opentelemetry::trace::SpanKind::Server);
        // Parent context was extracted from traceparent.
        assert_eq!(span.parent_span_id.to_string(), "00f067aa0ba902b7");
        let attr = |key: &str| {
            spans[0]
                .attributes
                .iter()
                .find(|kv| kv.key.as_str() == key)
                .map(|kv| kv.value.to_string())
        };
        assert_eq!(attr("rpc.system").as_deref(), Some("connectrpc"));
        assert_eq!(attr("http.response.status_code").as_deref(), Some("200"));
    }

    /// Minimal in-memory span exporter for assertions.
    #[derive(Debug, Default)]
    struct InMemoryExporter {
        spans: std::sync::Arc<std::sync::Mutex<Vec<opentelemetry_sdk::trace::SpanData>>>,
    }

    impl opentelemetry_sdk::trace::SpanExporter for InMemoryExporter {
        fn export(
            &self,
            batch: Vec<opentelemetry_sdk::trace::SpanData>,
        ) -> impl std::future::Future<Output = opentelemetry_sdk::error::OTelSdkResult> + Send
        {
            self.spans.lock().expect("lock").extend(batch);
            std::future::ready(Ok(()))
        }
    }
}
