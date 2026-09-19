//! Unified HTTP client builder for Sunbeam services.
//!
//! [`ClientBuilder`] constructs a [`Client`] with optional resilience middleware
//! (retry, circuit breaker, cache), authentication, and TLS configuration. The
//! resulting client exposes REST, GraphQL, and (with the `client-connectrpc`
//! feature) ConnectRPC transports sharing the same underlying stack.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, Request, Response};
use tower::Layer;
use tower::util::BoxCloneService;

use super::auth::{AuthLayer, TokenProvider};
use super::circuit_breaker::{CircuitBreaker, CircuitBreakerLayer};
#[cfg(feature = "g2v-client-connectrpc")]
use super::connect::ConnectTransport;
use super::graphql::GraphqlClient;
use super::rest::{ClientError, RestClient};
use super::retry::{RetryLayer, RetryPolicy};
use super::transport::ReqwestService;
use crate::g2v::BoxError;
use crate::g2v::client::cache::CacheConfig;
#[cfg(feature = "g2v-cache")]
use crate::g2v::client::cache::ClientCacheLayer;

/// Errors that can occur while building a [`Client`].
#[derive(Debug, thiserror::Error)]
pub enum ClientBuilderError {
    /// The supplied base URL could not be parsed.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    /// The supplied header name is not valid HTTP.
    #[error("invalid header name: {0}")]
    InvalidHeaderName(String),
    /// The supplied header value is not valid HTTP.
    #[error("invalid header value: {0}")]
    InvalidHeaderValue(String),
    /// The underlying HTTP client could not be constructed.
    #[error("failed to build HTTP client: {0}")]
    Build(#[from] reqwest::Error),
    /// A generic boxed error occurred.
    #[error(transparent)]
    Other(#[from] BoxError),
}

/// Builder for the Sunbeam unified HTTP client.
#[derive(Clone, Default)]
pub struct ClientBuilder {
    base_url: String,
    timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    retry: Option<RetryPolicy>,
    circuit_breaker: Option<Arc<CircuitBreaker>>,
    cache: Option<CacheConfig>,
    default_headers: HeaderMap,
    auth: Option<Arc<dyn TokenProvider>>,
    root_certs: Vec<reqwest::Certificate>,
    identity: Option<reqwest::Identity>,
    danger_accept_invalid_certs: bool,
}

impl std::fmt::Debug for ClientBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientBuilder")
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .field("connect_timeout", &self.connect_timeout)
            .field("retry", &self.retry)
            .field("circuit_breaker", &self.circuit_breaker)
            .field("cache", &self.cache)
            .field("default_headers", &self.default_headers)
            .field("auth", &self.auth.as_ref().map(|_| "TokenProvider"))
            .field("root_certs", &self.root_certs.len())
            .field("identity", &self.identity.as_ref().map(|_| "Identity"))
            .field(
                "danger_accept_invalid_certs",
                &self.danger_accept_invalid_certs,
            )
            .finish()
    }
}

impl ClientBuilder {
    /// Create a new builder with the given base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            ..Default::default()
        }
    }

    /// Set the base URL for all requests.
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Set the overall request timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the TCP connect timeout.
    #[must_use]
    pub fn connect_timeout(mut self, connect_timeout: Duration) -> Self {
        self.connect_timeout = Some(connect_timeout);
        self
    }

    /// Enable retry with the given policy.
    #[must_use]
    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = Some(policy);
        self
    }

    /// Enable circuit breaker with the given shared breaker.
    #[must_use]
    pub fn circuit_breaker(mut self, breaker: Arc<CircuitBreaker>) -> Self {
        self.circuit_breaker = Some(breaker);
        self
    }

    /// Enable response caching with the given cache configuration.
    #[must_use]
    pub fn cache(mut self, config: CacheConfig) -> Self {
        self.cache = Some(config);
        self
    }

    /// Disable response caching.
    #[must_use]
    pub fn no_cache(mut self) -> Self {
        self.cache = None;
        self
    }

    /// Add a default header sent with every request.
    ///
    /// # Errors
    ///
    /// Returns [`ClientBuilderError::InvalidHeaderName`] or
    /// [`ClientBuilderError::InvalidHeaderValue`] if the supplied components
    /// are not valid HTTP.
    pub fn default_header<N, V>(mut self, name: N, value: V) -> Result<Self, ClientBuilderError>
    where
        N: TryInto<HeaderName>,
        N::Error: std::fmt::Display,
        V: TryInto<HeaderValue>,
        V::Error: std::fmt::Display,
    {
        let name = name
            .try_into()
            .map_err(|e| ClientBuilderError::InvalidHeaderName(e.to_string()))?;
        let value = value
            .try_into()
            .map_err(|e| ClientBuilderError::InvalidHeaderValue(e.to_string()))?;
        self.default_headers.append(name, value);
        Ok(self)
    }

    /// Set the authentication token provider.
    #[must_use]
    pub fn auth<P>(mut self, provider: P) -> Self
    where
        P: TokenProvider,
    {
        self.auth = Some(Arc::new(provider));
        self
    }

    /// Add a trusted root certificate.
    #[must_use]
    pub fn add_root_certificate(mut self, cert: reqwest::Certificate) -> Self {
        self.root_certs.push(cert);
        self
    }

    /// Set the client TLS identity.
    #[must_use]
    pub fn identity(mut self, identity: reqwest::Identity) -> Self {
        self.identity = Some(identity);
        self
    }

    /// Accept invalid TLS certificates.
    ///
    /// # Danger
    /// This disables certificate validation and should never be used in
    /// production.
    #[must_use]
    pub fn danger_accept_invalid_certs(mut self) -> Self {
        self.danger_accept_invalid_certs = true;
        self
    }

    /// Build the configured [`Client`].
    pub fn build(self) -> Result<Client, ClientBuilderError> {
        let base_url = reqwest::Url::parse(&self.base_url)
            .map_err(|e| ClientBuilderError::InvalidUrl(e.to_string()))?;

        let mut reqwest_builder = reqwest::Client::builder();
        if let Some(timeout) = self.timeout {
            reqwest_builder = reqwest_builder.timeout(timeout);
        }
        if let Some(connect_timeout) = self.connect_timeout {
            reqwest_builder = reqwest_builder.connect_timeout(connect_timeout);
        }
        for cert in self.root_certs {
            reqwest_builder = reqwest_builder.add_root_certificate(cert);
        }
        if let Some(identity) = self.identity {
            reqwest_builder = reqwest_builder.identity(identity);
        }
        if self.danger_accept_invalid_certs {
            reqwest_builder = reqwest_builder.danger_accept_invalid_certs(true);
        }

        let reqwest_client = reqwest_builder.build()?;

        // Build the middleware stack from the transport outwards:
        // CircuitBreaker -> Retry -> ClientCache -> Auth -> ReqwestService.
        //
        // Circuit breaker sits outermost so an open circuit fast-fails before
        // retry/cache/auth run. Retry sits below it so the breaker only sees
        // the final outcome after retries, not every attempt. Cache is below
        // retry so cached successes short-circuit the retry logic. Auth is just
        // above the transport so tokens are injected right before sending.
        let service = BoxCloneService::new(ReqwestService::new(reqwest_client));

        let service = if let Some(provider) = self.auth {
            BoxCloneService::new(AuthLayer::new(provider).layer(service))
        } else {
            service
        };

        #[cfg(feature = "g2v-cache")]
        let service = if let Some(config) = self.cache {
            BoxCloneService::new(ClientCacheLayer::new(config).layer(service))
        } else {
            service
        };

        let service = if let Some(policy) = self.retry {
            BoxCloneService::new(RetryLayer::new(policy).layer(service))
        } else {
            service
        };

        let service = if let Some(breaker) = self.circuit_breaker {
            BoxCloneService::new(CircuitBreakerLayer::new(breaker).layer(service))
        } else {
            service
        };

        let boxed = service;

        Ok(Client {
            service: std::sync::Arc::new(std::sync::Mutex::new(boxed)),
            base_url,
            default_headers: self.default_headers,
        })
    }
}

/// Type alias for the boxed, cloneable Tower service used by [`Client`].
pub type BoxedClientService = BoxCloneService<Request<Bytes>, Response<Bytes>, BoxError>;

/// Unified Sunbeam HTTP client.
///
/// Clone is cheap: the underlying Tower service is boxed, cloneable, and
/// shared behind a short-lived mutex used only to obtain a clone for each
/// request. This makes [`Client`] both [`Clone`] and [`Sync`].
#[derive(Clone)]
pub struct Client {
    service: std::sync::Arc<std::sync::Mutex<BoxedClientService>>,
    base_url: reqwest::Url,
    default_headers: HeaderMap,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl Client {
    /// Create a [`RestClient`] rooted at this client's base URL.
    pub fn rest(&self) -> RestClient {
        RestClient::new(
            self.clone(),
            self.base_url.clone(),
            self.default_headers.clone(),
        )
    }

    /// Create a [`GraphqlClient`] rooted at this client's base URL.
    pub fn graphql(&self) -> GraphqlClient {
        GraphqlClient::new(self.rest())
    }

    /// Create a ConnectRPC transport for the given base URI.
    ///
    /// Requires the `client-connectrpc` feature. The returned transport
    /// implements [`connectrpc::client::ClientTransport`] and can be passed
    /// directly to generated service clients.
    #[cfg(feature = "g2v-client-connectrpc")]
    pub fn connectrpc(&self, base_uri: http::Uri) -> ConnectTransport {
        ConnectTransport::new(self.clone(), base_uri)
    }

    /// Execute a single HTTP request through the full middleware stack.
    ///
    /// This is the low-level entry point used by the sub-clients. Every
    /// request gets an `x-request-id` header (generated when absent) and,
    /// with the `tracing` feature, W3C trace-context headers propagated from
    /// the current span.
    pub async fn execute(&self, mut req: Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
        use tower::ServiceExt;

        super::propagation::inject_request_headers(req.headers_mut());

        let service = match self.service.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                // A poisoned client-service lock still holds a usable service;
                // recover it rather than failing every request.
                tracing::warn!("client service lock poisoned; recovering");
                poisoned.into_inner()
            }
        }
        .clone();

        #[cfg(feature = "g2v-server")]
        {
            use tracing::Instrument as _;

            let span = super::propagation::client_span(&req);
            let handle = span.clone();
            let result = service.oneshot(req).instrument(span).await;
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
                    handle.record("exception.message", e.to_string().as_str());
                }
            }
            result.map_err(ClientError::Transport)
        }

        #[cfg(not(feature = "g2v-server"))]
        {
            service.oneshot(req).await.map_err(ClientError::Transport)
        }
    }

    /// Return the base URL.
    pub fn base_url(&self) -> &reqwest::Url {
        &self.base_url
    }

    /// Return the default headers.
    pub fn default_headers(&self) -> &HeaderMap {
        &self.default_headers
    }
}

#[cfg(test)]
impl Client {
    /// Build a client directly from a boxed service, bypassing the builder.
    /// Used by unit tests to inject mock transports.
    pub fn from_service(
        service: BoxedClientService,
        base_url: reqwest::Url,
        default_headers: http::HeaderMap,
    ) -> Self {
        Self {
            service: std::sync::Arc::new(std::sync::Mutex::new(service)),
            base_url,
            default_headers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_builder_defaults() {
        let client = ClientBuilder::new("http://example.com").build().unwrap();
        assert_eq!(client.base_url().as_str(), "http://example.com/");
    }

    #[test]
    fn test_client_builder_invalid_url() {
        let err = ClientBuilder::new("://not-a-url").build().unwrap_err();
        assert!(matches!(err, ClientBuilderError::InvalidUrl(_)));
    }

    #[test]
    fn test_client_builder_default_header() {
        let client = ClientBuilder::new("http://example.com")
            .default_header("x-custom", "value")
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(client.default_headers().get("x-custom").unwrap(), "value");
    }

    #[test]
    fn test_client_builder_invalid_header_name() {
        let err = ClientBuilder::new("http://example.com")
            .default_header("\0", "value")
            .unwrap_err();
        assert!(matches!(err, ClientBuilderError::InvalidHeaderName(_)));
    }

    #[test]
    fn test_client_is_clone_and_sync() {
        fn assert_sync<T: Sync + Clone>(_t: T) {}
        let client = ClientBuilder::new("http://example.com").build().unwrap();
        assert_sync(client);
    }

    #[test]
    fn test_client_builder_debug() {
        let builder = ClientBuilder::new("http://example.com");
        let debug = format!("{builder:?}");
        assert!(debug.contains("ClientBuilder"));
        assert!(debug.contains("http://example.com"));
    }

    #[test]
    fn test_client_builder_full_chain() {
        use std::num::NonZeroUsize;
        use std::sync::Arc;
        use std::time::Duration;

        use crate::g2v::client::cache::CacheConfig;
        use crate::g2v::client::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
        use crate::g2v::client::retry::RetryPolicy;

        let cb = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 1,
            open_duration: Duration::from_millis(100),
            half_open_max_calls: 1,
        }));
        let cache = CacheConfig {
            capacity: NonZeroUsize::new(10).unwrap(),
            ttl: Some(Duration::from_secs(10)),
            max_body_size: 1024,
        };
        let retry = RetryPolicy {
            max_attempts: 2,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(100),
            jitter: false,
            retry_unauthenticated: false,
        };

        let client = ClientBuilder::new("http://example.com")
            .base_url("http://example.com/api")
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(1))
            .retry(retry)
            .circuit_breaker(Arc::clone(&cb))
            .cache(cache)
            .no_cache()
            .default_header("x-foo", "bar")
            .unwrap()
            .auth(crate::g2v::client::BearerToken::new("tok"))
            .danger_accept_invalid_certs()
            .build()
            .unwrap();

        assert_eq!(client.base_url().as_str(), "http://example.com/api");
        assert_eq!(
            client
                .default_headers()
                .get("x-foo")
                .and_then(|v| v.to_str().ok()),
            Some("bar")
        );
        let debug = format!("{client:?}");
        assert!(debug.starts_with("Client { base_url: Url {"));
        assert!(debug.contains("example.com"));
    }

    #[test]
    fn test_client_builder_invalid_header_value() {
        let err = ClientBuilder::new("http://example.com")
            .default_header("x-custom", "\0")
            .unwrap_err();
        assert!(matches!(err, ClientBuilderError::InvalidHeaderValue(_)));
    }

    #[tokio::test]
    async fn test_client_execute_via_mock_service() {
        use bytes::Bytes;
        use http::{Request, Response};

        let service = tower::service_fn(|req: Request<Bytes>| async move {
            assert_eq!(req.uri().path(), "/hello");
            Ok::<_, crate::g2v::BoxError>(Response::new(Bytes::from_static(b"world")))
        });

        let client = Client::from_service(
            super::BoxedClientService::new(service),
            reqwest::Url::parse("http://example.com").unwrap(),
            http::HeaderMap::new(),
        );

        let req = Request::get("http://example.com/hello")
            .body(Bytes::new())
            .unwrap();
        let resp = client.execute(req).await.unwrap();
        assert_eq!(resp.body().as_ref(), b"world");
    }
}
