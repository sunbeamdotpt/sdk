//! Request ID middleware — honors or generates `x-request-id` on every
//! request and echoes it back on the response.
//!
//! Applied by default to every [`AxumServer`](crate::g2v::server::axum::AxumServer)
//! route. The id is also exposed to handlers via request extensions as
//! [`RequestId`], and recorded onto the active tracing span (with the
//! `tracing` feature) so logs and traces can be correlated by request id.

use tower::{Layer, Service};
use uuid::Uuid;

/// Header carrying the request id.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Maximum accepted length for a client-supplied request id. Longer values
/// are discarded and replaced with a generated id.
const MAX_REQUEST_ID_LEN: usize = 128;

/// Request id for the in-flight request, stored in request extensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(pub String);

impl RequestId {
    /// Get the request id from a request's extensions, falling back to its
    /// headers.
    pub fn extract<B>(req: &http::Request<B>) -> Option<String> {
        req.extensions()
            .get::<RequestId>()
            .map(|id| id.0.clone())
            .or_else(|| get_request_id(req.headers()))
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Request ID middleware layer.
#[derive(Debug, Clone, Copy, Default)]
pub struct RequestIdLayer;

impl<S> Layer<S> for RequestIdLayer {
    type Service = RequestIdService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestIdService { inner }
    }
}

/// Request ID middleware service.
#[derive(Debug, Clone)]
pub struct RequestIdService<S> {
    inner: S,
}

/// Return a usable client-supplied request id, if present and sane.
fn incoming_request_id(headers: &http::HeaderMap) -> Option<String> {
    let value = headers.get(REQUEST_ID_HEADER)?.to_str().ok()?;
    if value.is_empty() || value.len() > MAX_REQUEST_ID_LEN {
        return None;
    }
    Some(value.to_string())
}

/// Generate a fresh UUIDv4 request id plus its header value. The header
/// value is `None` only in the unreachable case that a UUID string is not a
/// valid header value; the caller passes the request through untouched then.
fn generated_request_id() -> (String, Option<http::HeaderValue>) {
    let id = Uuid::new_v4().to_string();
    let value = http::HeaderValue::from_str(&id).ok();
    (id, value)
}

impl<S, B, ResB> Service<http::Request<B>> for RequestIdService<S>
where
    S: Service<http::Request<B>, Response = http::Response<ResB>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
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

    fn call(&mut self, mut req: http::Request<B>) -> Self::Future {
        // Honor a sane client-supplied id; otherwise (or if it cannot
        // round-trip into a header value) generate a fresh UUIDv4.
        let (request_id, header_value) = match incoming_request_id(req.headers()) {
            Some(id) => match http::HeaderValue::from_str(&id) {
                Ok(value) => (id, Some(value)),
                Err(e) => {
                    // The id survived `to_str` but cannot round-trip into a
                    // header value (e.g. obs-text bytes). Replace it.
                    #[cfg(feature = "g2v-server")]
                    tracing::debug!(msg = "replacing unusable client request id", error = %e);
                    let _ = e;
                    generated_request_id()
                }
            },
            None => generated_request_id(),
        };

        let Some(header_value) = header_value else {
            // Unreachable: UUIDv4 strings are always valid header values.
            // Without an id to propagate, pass the request through untouched.
            return Box::pin(self.inner.call(req));
        };

        req.headers_mut()
            .insert(REQUEST_ID_HEADER, header_value.clone());
        req.extensions_mut().insert(RequestId(request_id.clone()));

        // Correlate the request id with the active tracing span (no-op when
        // no span is active or the span does not declare the field).
        #[cfg(feature = "g2v-server")]
        tracing::Span::current().record("request_id", request_id.as_str());

        let fut = self.inner.call(req);
        Box::pin(async move {
            let mut response = fut.await?;
            response
                .headers_mut()
                .insert(REQUEST_ID_HEADER, header_value);
            Ok(response)
        })
    }
}

/// Get the request ID from headers.
pub fn get_request_id(headers: &http::HeaderMap) -> Option<String> {
    headers
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inner service that echoes the observed request headers/extensions back.
    #[allow(clippy::type_complexity)]
    fn echo_service() -> tower::util::ServiceFn<
        impl Fn(
            http::Request<()>,
        ) -> std::future::Ready<Result<http::Response<()>, std::convert::Infallible>>
        + Clone,
    > {
        tower::service_fn(|req: http::Request<()>| {
            let observed_header = get_request_id(req.headers());
            let observed_ext = req.extensions().get::<RequestId>().cloned();
            let mut response = http::Response::new(());
            if let Some(id) = observed_header {
                response.headers_mut().insert(
                    "x-observed-request-id",
                    http::HeaderValue::from_str(&id).expect("uuid is a valid header value"),
                );
            }
            if let Some(id) = observed_ext {
                response.headers_mut().insert(
                    "x-observed-extension",
                    http::HeaderValue::from_str(&id.0).expect("uuid is a valid header value"),
                );
            }
            std::future::ready(Ok::<_, std::convert::Infallible>(response))
        })
    }

    #[tokio::test]
    async fn test_generates_request_id_when_absent() {
        let mut service = RequestIdLayer.layer(echo_service());
        let response = Service::call(&mut service, http::Request::new(()))
            .await
            .expect("inner service is infallible");

        let echoed = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .expect("response echoes x-request-id");
        assert!(uuid::Uuid::parse_str(echoed).is_ok(), "echoed id is a uuid");
        assert_eq!(
            response.headers().get("x-observed-request-id").unwrap(),
            echoed,
            "inner service saw the same id on the request"
        );
        assert_eq!(
            response.headers().get("x-observed-extension").unwrap(),
            echoed,
            "inner service saw the same id in extensions"
        );
    }

    #[tokio::test]
    async fn test_preserves_incoming_request_id() {
        let mut service = RequestIdLayer.layer(echo_service());
        let mut request = http::Request::new(());
        request.headers_mut().insert(
            REQUEST_ID_HEADER,
            http::HeaderValue::from_static("client-supplied-id"),
        );
        let response = Service::call(&mut service, request)
            .await
            .expect("inner service is infallible");

        assert_eq!(
            response.headers().get(REQUEST_ID_HEADER).unwrap(),
            "client-supplied-id"
        );
        assert_eq!(
            response.headers().get("x-observed-request-id").unwrap(),
            "client-supplied-id"
        );
    }

    #[tokio::test]
    async fn test_replaces_oversized_request_id() {
        let mut service = RequestIdLayer.layer(echo_service());
        let mut request = http::Request::new(());
        let oversized = "x".repeat(MAX_REQUEST_ID_LEN + 1);
        request.headers_mut().insert(
            REQUEST_ID_HEADER,
            http::HeaderValue::from_str(&oversized).unwrap(),
        );
        let response = Service::call(&mut service, request)
            .await
            .expect("inner service is infallible");

        let echoed = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap();
        assert_ne!(echoed, oversized);
        assert!(uuid::Uuid::parse_str(echoed).is_ok());
    }

    #[test]
    fn test_get_request_id() {
        let mut headers = http::HeaderMap::new();
        headers.insert(REQUEST_ID_HEADER, http::HeaderValue::from_static("test-id"));
        assert_eq!(get_request_id(&headers), Some("test-id".to_string()));
    }
}
