//! Outbound request propagation — `x-request-id` and W3C trace context.
//!
//! [`Client::execute`](super::builder::Client::execute) calls
//! [`inject_request_headers`] on every request, so all sub-clients (REST,
//! GraphQL, ConnectRPC) propagate correlation metadata by default:
//!
//! - `x-request-id` is generated when the caller did not set one.
//! - With the `tracing` feature, the current span's trace context is injected
//!   as `traceparent` / `tracestate`, connecting outbound calls to the
//!   in-flight server trace.

#[cfg(feature = "g2v-server")]
use bytes::Bytes;
use http::HeaderMap;
#[cfg(feature = "g2v-server")]
use http::Request;

/// Header carrying the request id (kept in sync with the server
/// middleware's `REQUEST_ID_HEADER`; duplicated so the client stack does not
/// depend on server modules).
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Inject `x-request-id` and (with the `tracing` feature) W3C trace-context
/// headers into an outbound request. Existing headers are never overwritten.
pub fn inject_request_headers(headers: &mut HeaderMap) {
    if !headers.contains_key(REQUEST_ID_HEADER) {
        let id = uuid::Uuid::new_v4().to_string();
        match http::HeaderValue::from_str(&id) {
            Ok(value) => {
                headers.insert(REQUEST_ID_HEADER, value);
            }
            Err(e) => {
                // Unreachable: UUIDv4 strings are always valid header values.
                #[cfg(feature = "g2v-server")]
                tracing::warn!(msg = "failed to build request id header", error = %e);
                let _ = e;
            }
        }
    }

    #[cfg(feature = "g2v-server")]
    {
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;

        let context = tracing::Span::current().context();
        opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.inject_context(&context, &mut HeaderInjector(headers));
        });
    }
}

/// Build a client span for an outbound request (feature `tracing`).
///
/// The span wraps the send so the outbound call shows up as a `client` span
/// in distributed traces; `x-request-id` must already be injected so it can
/// be recorded here.
#[cfg(feature = "g2v-server")]
pub fn client_span(req: &Request<Bytes>) -> tracing::Span {
    let request_id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    tracing::info_span!(
        "http.client",
        "otel.kind" = "client",
        "http.request.method" = %req.method(),
        "url.full" = %req.uri(),
        "server.address" = req.uri().host(),
        "request_id" = request_id,
        "http.response.status_code" = tracing::field::Empty,
        "otel.status_code" = tracing::field::Empty,
    )
}

/// Injector adapter over request headers for the OTel propagator.
#[cfg(feature = "g2v-server")]
struct HeaderInjector<'a>(&'a mut HeaderMap);

#[cfg(feature = "g2v-server")]
impl opentelemetry::propagation::Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        let Ok(name) = http::header::HeaderName::try_from(key) else {
            return;
        };
        let Ok(value) = http::HeaderValue::from_str(&value) else {
            return;
        };
        self.0.insert(name, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_injects_request_id_when_absent() {
        let mut headers = HeaderMap::new();
        inject_request_headers(&mut headers);
        let value = headers
            .get(REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .expect("x-request-id injected");
        assert!(uuid::Uuid::parse_str(value).is_ok());
    }

    #[test]
    fn test_preserves_existing_request_id() {
        let mut headers = HeaderMap::new();
        headers.insert(
            REQUEST_ID_HEADER,
            http::HeaderValue::from_static("caller-set"),
        );
        inject_request_headers(&mut headers);
        assert_eq!(headers.get(REQUEST_ID_HEADER).unwrap(), "caller-set");
    }

    #[cfg(feature = "g2v-server")]
    #[test]
    fn test_client_span_uses_request_id() {
        let req = Request::builder()
            .method("POST")
            .uri("https://example.com/pkg.Service/Method")
            .header(REQUEST_ID_HEADER, "request-123")
            .body(Bytes::new())
            .unwrap();
        let span = client_span(&req);
        drop(span);
    }

    #[cfg(feature = "g2v-server")]
    #[test]
    fn test_header_injector_ignores_invalid_names_and_values() {
        use opentelemetry::propagation::Injector as _;

        let mut headers = HeaderMap::new();
        {
            let mut injector = HeaderInjector(&mut headers);
            injector.set("not a header", "value".to_string());
            injector.set("x-valid", "bad\nvalue".to_string());
            injector.set("x-valid", "good".to_string());
        }
        assert_eq!(headers.get("x-valid").unwrap(), "good");
    }
}
