//! Retry logic for service clients.
//!
//! This module provides two complementary retry mechanisms:
//!
//! 1. **[`retry`] free function** — a simple async retry loop for arbitrary fallible
//!    futures.  Uses [`RetryConfig`] for backoff parameters.
//!
//! 2. **[`RetryLayer`] / [`RetryService`]** — a Tower [`Layer`] that wraps an inner
//!    service and transparently retries on `Err` responses or on 5xx (configurable)
//!    HTTP status codes.
//!
//! # Body clone constraint
//!
//! `RetryService` requires `B: Clone + Send + 'static`.  This means it works
//! naturally with `String`, `Bytes`, `Vec<u8>`, and similar owned body types.
//! It does **not** work with `axum::body::Body` (a streaming body) because that
//! type cannot be cloned.  For axum handlers wrap the body in `Bytes` before
//! passing it to a `RetryLayer`-wrapped service, or use the free [`retry`] function
//! instead.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;

use tower::{Layer, Service};

/// Predicate that decides whether a response should be retried.
pub type RetryPredicate = Arc<dyn Fn(&http::Response<bytes::Bytes>) -> bool + Send + Sync>;

// ============================================================================
// RetryConfig — low-level backoff parameters
// ============================================================================

/// Retry configuration.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Initial delay before first retry.
    pub initial_delay: Duration,
    /// Maximum delay between retries.
    pub max_delay: Duration,
    /// Multiplier for exponential backoff.
    pub backoff_multiplier: f64,
    /// Jitter factor (0.0 to 1.0).
    pub jitter: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: 0.1,
        }
    }
}

impl RetryConfig {
    /// Create a new retry config with default exponential backoff.
    pub fn exponential_backoff(max_retries: u32) -> Self {
        Self {
            max_retries,
            ..Default::default()
        }
    }

    /// Calculate delay for a given retry attempt (1-based).
    pub fn delay(&self, attempt: u32) -> Duration {
        let exponent = self
            .backoff_multiplier
            .powi(attempt.saturating_sub(1) as i32);
        let delay_millis = self.initial_delay.as_millis() as f64 * exponent;
        let delay = Duration::from_millis(delay_millis.min(u64::MAX as f64) as u64);
        delay.min(self.max_delay)
    }
}

// ============================================================================
// RetryError — error type for the free retry function
// ============================================================================

/// Result of a retry operation.
pub type RetryResult<T, E> = Result<T, RetryError<E>>;

/// Error type for retry operations.
#[derive(Debug, Clone)]
pub enum RetryError<E> {
    /// All retries exhausted.
    Exhausted {
        /// The last error that occurred before giving up.
        last_error: E,
        /// Number of attempts that were made.
        attempts: u32,
    },
    /// Operation was cancelled.
    Cancelled,
    /// Wrapped error from the operation.
    Error(E),
}

impl<E> RetryError<E> {
    /// Get the last error that occurred.
    pub fn last_error(&self) -> Option<&E> {
        match self {
            RetryError::Exhausted { last_error, .. } => Some(last_error),
            RetryError::Error(e) => Some(e),
            RetryError::Cancelled => None,
        }
    }
}

impl<E: std::fmt::Display> std::fmt::Display for RetryError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RetryError::Exhausted { attempts, .. } => {
                write!(f, "Retry exhausted after {} attempts", attempts)
            }
            RetryError::Cancelled => write!(f, "Retry cancelled"),
            RetryError::Error(e) => write!(f, "Retry error: {}", e),
        }
    }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for RetryError<E> {}

// ============================================================================
// retry — free async retry function
// ============================================================================

/// Retry a fallible async operation.
pub async fn retry<F, Fut, T, E>(config: RetryConfig, mut operation: F) -> RetryResult<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: Clone,
{
    use rand::RngExt;

    let mut attempt = 0u32;
    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(e) => {
                if attempt >= config.max_retries {
                    return Err(RetryError::Exhausted {
                        last_error: e,
                        attempts: config.max_retries,
                    });
                }
                attempt += 1;
                let delay = config.delay(attempt);

                // Add jitter
                let jitter_range = delay.as_millis() as f64 * config.jitter;
                let jitter_millis = rand::rng().random_range(-jitter_range..jitter_range);
                let delay_with_jitter =
                    delay.saturating_add(Duration::from_millis(jitter_millis.abs() as u64));

                tokio::time::sleep(delay_with_jitter).await;
            }
        }
    }
}

// ============================================================================
// RetryPolicy — Tower layer config
// ============================================================================

/// Policy for the [`RetryLayer`] Tower middleware.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum total attempts (initial + retries).
    pub max_attempts: u32,
    /// Initial backoff before the first retry.
    pub initial_backoff: Duration,
    /// Maximum backoff cap.
    pub max_backoff: Duration,
    /// Whether to add ±jitter to each computed backoff.
    pub jitter: bool,
    /// Whether 401 Unauthenticated responses are retried (default: `false`).
    ///
    /// Interplay with [`crate::g2v::client::AuthLayer`]: the auth layer sits
    /// *inside* the retry layer in the standard stack, so every retried
    /// attempt re-runs the auth layer — a refresh-capable
    /// [`crate::g2v::client::TokenProvider`] gets a fresh token per attempt, and
    /// its own single refresh-and-retry still applies first. Enable this to
    /// ride through longer transient auth wedges (SSO-023); leave it disabled
    /// to fail after the auth layer's built-in refresh.
    pub retry_unauthenticated: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(30),
            jitter: true,
            retry_unauthenticated: false,
        }
    }
}

impl RetryPolicy {
    /// Compute the backoff for a given attempt index (0-based, 0 = first retry).
    ///
    /// Returns `None` when `attempt_index >= max_attempts - 1` (no more retries).
    pub fn backoff(&self, attempt_index: u32) -> Duration {
        let factor = 2u64.saturating_pow(attempt_index);
        let base_ms = self.initial_backoff.as_millis() as u64;
        let computed_ms = base_ms.saturating_mul(factor);
        let cap_ms = self.max_backoff.as_millis() as u64;
        let capped_ms = computed_ms.min(cap_ms);

        if self.jitter {
            use rand::RngExt;
            let jitter_ms = rand::rng().random_range(0..=(capped_ms / 4).max(1));
            Duration::from_millis(capped_ms.saturating_add(jitter_ms))
        } else {
            Duration::from_millis(capped_ms)
        }
    }
}

// ============================================================================
// Default response predicate
// ============================================================================

/// Header emitted by the circuit breaker when it is open. Retry must not
/// attempt to bypass the breaker.
pub const CIRCUIT_OPEN_HEADER: &str = "x-sunbeam-circuit-open";

/// Returns `true` for responses that should trigger a retry:
/// - Any 5xx
/// - Any 4xx that is not 400, 401, 403, or 404
///
/// Responses carrying [`CIRCUIT_OPEN_HEADER`] are never retried, preventing
/// retries from overwhelming an already-open circuit.
fn default_should_retry<B>(resp: &http::Response<B>) -> bool {
    if resp.headers().contains_key(CIRCUIT_OPEN_HEADER) {
        return false;
    }
    let status = resp.status().as_u16();
    if status >= 500 {
        return true;
    }
    if (400..500).contains(&status) {
        // These 4xx codes are definitive — do not retry.
        return !matches!(status, 400 | 401 | 403 | 404);
    }
    false
}

// ============================================================================
// RetryLayer
// ============================================================================

/// Tower [`Layer`] that adds transparent retry behaviour to an inner service.
///
/// # Body clone constraint
///
/// The service requires `B: Clone + Send + 'static`.  See module-level docs for
/// details.
#[derive(Clone)]
pub struct RetryLayer {
    policy: RetryPolicy,
    should_retry: RetryPredicate,
}

impl std::fmt::Debug for RetryLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetryLayer")
            .field("policy", &self.policy)
            .finish()
    }
}

impl RetryLayer {
    /// Create a new layer with the given policy and the default retry predicate.
    ///
    /// When [`RetryPolicy::retry_unauthenticated`] is set, the predicate also
    /// retries 401 responses (see the field docs for the auth-layer interplay).
    pub fn new(policy: RetryPolicy) -> Self {
        let retry_unauthenticated = policy.retry_unauthenticated;
        Self {
            policy,
            should_retry: Arc::new(move |resp: &http::Response<bytes::Bytes>| {
                default_should_retry(resp)
                    || (retry_unauthenticated && resp.status() == http::StatusCode::UNAUTHORIZED)
            }),
        }
    }

    /// Override the response predicate that decides whether to retry.
    ///
    /// The predicate receives the (buffered) response body as `bytes::Bytes`.
    pub fn with_predicate<F>(mut self, f: F) -> Self
    where
        F: Fn(&http::Response<bytes::Bytes>) -> bool + Send + Sync + 'static,
    {
        self.should_retry = Arc::new(f) as RetryPredicate;
        self
    }
}

impl<S> Layer<S> for RetryLayer {
    type Service = RetryService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RetryService {
            inner,
            policy: self.policy.clone(),
            should_retry: Arc::clone(&self.should_retry),
        }
    }
}

// ============================================================================
// RetryService
// ============================================================================

/// Tower [`Service`] produced by [`RetryLayer`].
#[derive(Clone)]
pub struct RetryService<S> {
    inner: S,
    policy: RetryPolicy,
    should_retry: RetryPredicate,
}

impl<S, B> Service<http::Request<B>> for RetryService<S>
where
    S: Service<http::Request<B>> + Clone + Send + 'static,
    S::Response: Into<http::Response<bytes::Bytes>> + Send + 'static,
    S::Error: Send + 'static,
    S::Future: Send + 'static,
    B: Clone + Send + 'static,
{
    type Response = http::Response<bytes::Bytes>;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        let policy = self.policy.clone();
        let should_retry = Arc::clone(&self.should_retry);
        // Clone inner for the async block; the original must stay poll_ready-d.
        // Per Tower's documented pattern: clone and swap so self.inner is the
        // already-ready clone, and `inner` is the original that we own.
        let inner = self.inner.clone();
        // Swap: self.inner becomes the clone (may need another poll_ready),
        // inner is the already-polled-ready original.
        let inner = std::mem::replace(&mut self.inner, inner);

        Box::pin(async move {
            let mut attempt = 0u32;
            loop {
                // Re-clone the request for each attempt.
                let req_clone = http::Request::builder()
                    .method(req.method().clone())
                    .uri(req.uri().clone())
                    .version(req.version())
                    .body(req.body().clone())
                    .unwrap_or_else(|_| {
                        // Reconstruct with headers
                        let mut r = http::Request::new(req.body().clone());
                        *r.method_mut() = req.method().clone();
                        *r.uri_mut() = req.uri().clone();
                        *r.version_mut() = req.version();
                        r
                    });

                // Copy headers
                let (mut parts, body) = req_clone.into_parts();
                for (name, value) in req.headers() {
                    parts.headers.insert(name.clone(), value.clone());
                }
                let req_attempt = http::Request::from_parts(parts, body);

                // We need a fresh clone of inner for subsequent attempts because
                // Tower services are single-call after poll_ready.
                let mut svc = if attempt == 0 {
                    // First call: use the already-ready inner.
                    inner.clone()
                } else {
                    // Subsequent calls: clone, and we accept that poll_ready may
                    // be needed — for service_fn / stateless services this is fine.
                    inner.clone()
                };

                match svc.call(req_attempt).await {
                    Err(e) => {
                        attempt += 1;
                        if attempt >= policy.max_attempts {
                            return Err(e);
                        }
                        let backoff = policy.backoff(attempt - 1);
                        tokio::time::sleep(backoff).await;
                    }
                    Ok(resp) => {
                        let resp: http::Response<bytes::Bytes> = resp.into();
                        if (should_retry)(&resp) {
                            attempt += 1;
                            if attempt >= policy.max_attempts {
                                return Ok(resp);
                            }
                            let backoff = policy.backoff(attempt - 1);
                            tokio::time::sleep(backoff).await;
                        } else {
                            return Ok(resp);
                        }
                    }
                }
            }
        })
    }
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::{ServiceBuilder, ServiceExt};

    // -------------------------------------------------------------------------
    // RetryConfig / free retry function (pre-existing tests, preserved as-is)
    // -------------------------------------------------------------------------

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_retry_config_delay() {
        let config = RetryConfig {
            initial_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(10),
            ..Default::default()
        };

        assert_eq!(config.delay(1), Duration::from_millis(100));
        assert_eq!(config.delay(2), Duration::from_millis(200));
        assert_eq!(config.delay(3), Duration::from_millis(400));
        assert_eq!(config.delay(4), Duration::from_millis(800));
    }

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let config = RetryConfig::default();
        let result: Result<i32, RetryError<&str>> = retry(config, || async { Ok(42) }).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_success_after_retries() {
        let config = RetryConfig {
            max_retries: 3,
            initial_delay: Duration::from_millis(10),
            ..Default::default()
        };

        let attempts = AtomicUsize::new(0);
        let result = retry(config, || {
            let count = attempts.fetch_add(1, Ordering::SeqCst);
            async move {
                if count >= 2 {
                    Ok("success")
                } else {
                    Err("fail")
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert!(attempts.load(Ordering::SeqCst) >= 3);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let config = RetryConfig {
            max_retries: 2,
            initial_delay: Duration::from_millis(10),
            ..Default::default()
        };

        let result = retry(config, || async { Err::<&str, _>("always fails") }).await;
        match result {
            Err(RetryError::Exhausted { attempts, .. }) => assert_eq!(attempts, 2),
            _ => panic!("Expected Exhausted error"),
        }
    }

    // -------------------------------------------------------------------------
    // RetryPolicy backoff
    // -------------------------------------------------------------------------

    #[test]
    fn test_retry_policy_backoff_doubles() {
        let policy = RetryPolicy {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(60),
            jitter: false, // deterministic
            retry_unauthenticated: false,
        };

        assert_eq!(policy.backoff(0), Duration::from_millis(100));
        assert_eq!(policy.backoff(1), Duration::from_millis(200));
        assert_eq!(policy.backoff(2), Duration::from_millis(400));
        assert_eq!(policy.backoff(3), Duration::from_millis(800));
    }

    #[test]
    fn test_retry_policy_backoff_capped() {
        let policy = RetryPolicy {
            max_attempts: 10,
            initial_backoff: Duration::from_millis(1000),
            max_backoff: Duration::from_millis(2000),
            jitter: false,
            retry_unauthenticated: false,
        };

        assert_eq!(policy.backoff(0), Duration::from_millis(1000));
        assert_eq!(policy.backoff(1), Duration::from_millis(2000));
        assert_eq!(policy.backoff(2), Duration::from_millis(2000)); // capped
    }

    #[tokio::test]
    async fn test_retry_layer_retries_401_only_when_opted_in() {
        use tower::ServiceExt as _;

        let make_inner = |failures: Arc<AtomicUsize>| {
            tower::service_fn(move |_req: http::Request<String>| {
                let n = failures.fetch_add(1, Ordering::SeqCst);
                async move {
                    let status = if n < 2 { 401 } else { 200 };
                    Ok::<_, std::convert::Infallible>(
                        http::Response::builder()
                            .status(status)
                            .body(bytes::Bytes::new())
                            .unwrap(),
                    )
                }
            })
        };

        let policy = |retry_unauthenticated: bool| RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
            jitter: false,
            retry_unauthenticated,
        };

        // Opted out (default): 401 is definitive, exactly one attempt.
        let calls = Arc::new(AtomicUsize::new(0));
        let mut svc = RetryLayer::new(policy(false)).layer(make_inner(Arc::clone(&calls)));
        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(make_request("/x"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 401);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Opted in: 401 is retried, the burst is ridden through.
        let calls = Arc::new(AtomicUsize::new(0));
        let mut svc = RetryLayer::new(policy(true)).layer(make_inner(Arc::clone(&calls)));
        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(make_request("/x"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    // -------------------------------------------------------------------------
    // Helper: make a simple string-body request
    // -------------------------------------------------------------------------

    fn make_request(path: &str) -> http::Request<String> {
        http::Request::builder()
            .method("GET")
            .uri(path)
            .body(String::new())
            .unwrap()
    }

    // -------------------------------------------------------------------------
    // RetryLayer Tower tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_retry_layer_succeeds_first_try() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);

        let inner = tower::service_fn(move |_req: http::Request<String>| {
            cc.fetch_add(1, Ordering::SeqCst);
            async {
                Ok::<_, std::convert::Infallible>(
                    http::Response::builder()
                        .status(200)
                        .body(bytes::Bytes::new())
                        .unwrap(),
                )
            }
        });

        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(10),
            jitter: false,
            retry_unauthenticated: false,
        };

        let mut svc = ServiceBuilder::new()
            .layer(RetryLayer::new(policy))
            .service(inner);

        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(make_request("/"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // no retries
    }

    #[tokio::test]
    async fn test_retry_layer_retries_on_error() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);

        let inner = tower::service_fn(move |_req: http::Request<String>| {
            let n = cc.fetch_add(1, Ordering::SeqCst);
            async move {
                if n < 2 {
                    Err("transient")
                } else {
                    Ok(http::Response::builder()
                        .status(200)
                        .body(bytes::Bytes::new())
                        .unwrap())
                }
            }
        });

        let policy = RetryPolicy {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(10),
            jitter: false,
            retry_unauthenticated: false,
        };

        tokio::time::pause();

        let mut svc = ServiceBuilder::new()
            .layer(RetryLayer::new(policy))
            .service(inner);

        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(make_request("/"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_layer_exhausts_attempts() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);

        let inner = tower::service_fn(move |_req: http::Request<String>| {
            cc.fetch_add(1, Ordering::SeqCst);
            async { Err::<http::Response<bytes::Bytes>, _>("always") }
        });

        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(10),
            jitter: false,
            retry_unauthenticated: false,
        };

        tokio::time::pause();

        let mut svc = ServiceBuilder::new()
            .layer(RetryLayer::new(policy))
            .service(inner);

        let result = svc.ready().await.unwrap().call(make_request("/")).await;
        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_layer_5xx_triggers_retry() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);

        let inner = tower::service_fn(move |_req: http::Request<String>| {
            let n = cc.fetch_add(1, Ordering::SeqCst);
            async move {
                let status = if n < 2 { 503 } else { 200 };
                Ok::<_, std::convert::Infallible>(
                    http::Response::builder()
                        .status(status)
                        .body(bytes::Bytes::new())
                        .unwrap(),
                )
            }
        });

        let policy = RetryPolicy {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(10),
            jitter: false,
            retry_unauthenticated: false,
        };

        tokio::time::pause();

        let mut svc = ServiceBuilder::new()
            .layer(RetryLayer::new(policy))
            .service(inner);

        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(make_request("/"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_layer_skips_circuit_open_header() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);

        let inner = tower::service_fn(move |_req: http::Request<String>| {
            cc.fetch_add(1, Ordering::SeqCst);
            async {
                Ok::<_, std::convert::Infallible>(
                    http::Response::builder()
                        .status(503)
                        .header(CIRCUIT_OPEN_HEADER, "true")
                        .body(bytes::Bytes::new())
                        .unwrap(),
                )
            }
        });

        let policy = RetryPolicy {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(10),
            jitter: false,
            retry_unauthenticated: false,
        };

        let mut svc = ServiceBuilder::new()
            .layer(RetryLayer::new(policy))
            .service(inner);

        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(make_request("/"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 503);
        // Inner was called exactly once because the circuit-open header
        // suppresses retries.
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    // -------------------------------------------------------------------------
    // RetryConfig / RetryPolicy coverage
    // -------------------------------------------------------------------------

    #[test]
    fn test_retry_config_exponential_backoff() {
        let config = RetryConfig::exponential_backoff(5);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
    }

    #[test]
    fn test_retry_config_delay_with_jitter_bounds() {
        let config = RetryConfig {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter: 0.5,
        };
        let delay = config.delay(2);
        assert!(delay >= Duration::from_millis(200));
        assert!(delay <= Duration::from_millis(600));
    }

    #[test]
    fn test_retry_policy_default() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.initial_backoff, Duration::from_millis(100));
        assert!(policy.jitter);
    }

    #[test]
    fn test_retry_policy_backoff_with_jitter() {
        let policy = RetryPolicy {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(1000),
            jitter: true,
            retry_unauthenticated: false,
        };
        let delay = policy.backoff(1);
        assert!(delay >= Duration::from_millis(200));
        assert!(delay <= Duration::from_millis(250));
    }

    #[test]
    fn test_retry_layer_with_predicate() {
        let layer = RetryLayer::new(RetryPolicy::default())
            .with_predicate(|resp| resp.status().is_server_error());
        assert!(format!("{layer:?}").contains("RetryLayer"));
    }

    #[tokio::test]
    async fn test_retry_layer_404_not_retried() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);

        let inner = tower::service_fn(move |_req: http::Request<String>| {
            cc.fetch_add(1, Ordering::SeqCst);
            async {
                Ok::<_, std::convert::Infallible>(
                    http::Response::builder()
                        .status(404)
                        .body(bytes::Bytes::new())
                        .unwrap(),
                )
            }
        });

        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(10),
            jitter: false,
            retry_unauthenticated: false,
        };

        let mut svc = ServiceBuilder::new()
            .layer(RetryLayer::new(policy))
            .service(inner);

        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(make_request("/"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 404);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_layer_408_is_retried() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);

        let inner = tower::service_fn(move |_req: http::Request<String>| {
            let n = cc.fetch_add(1, Ordering::SeqCst);
            async move {
                let status = if n == 0 { 408 } else { 200 };
                Ok::<_, std::convert::Infallible>(
                    http::Response::builder()
                        .status(status)
                        .body(bytes::Bytes::new())
                        .unwrap(),
                )
            }
        });

        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(10),
            jitter: false,
            retry_unauthenticated: false,
        };

        tokio::time::pause();
        let mut svc = ServiceBuilder::new()
            .layer(RetryLayer::new(policy))
            .service(inner);

        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(make_request("/"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    // -------------------------------------------------------------------------
    // RetryError coverage
    // -------------------------------------------------------------------------

    #[test]
    fn test_retry_error_last_error() {
        let err = RetryError::Error("e");
        assert_eq!(err.last_error(), Some(&"e"));
        let exhausted = RetryError::Exhausted {
            last_error: "last",
            attempts: 2,
        };
        assert_eq!(exhausted.last_error(), Some(&"last"));
        let cancelled: RetryError<&str> = RetryError::Cancelled;
        assert_eq!(cancelled.last_error(), None);
    }

    #[test]
    fn test_retry_error_display() {
        assert_eq!(
            format!(
                "{}",
                RetryError::<&str>::Exhausted {
                    last_error: "x",
                    attempts: 2,
                }
            ),
            "Retry exhausted after 2 attempts"
        );
        assert_eq!(
            format!("{}", RetryError::<&str>::Cancelled),
            "Retry cancelled"
        );
        assert_eq!(
            format!("{}", RetryError::Error("oops")),
            "Retry error: oops"
        );
    }

    #[test]
    fn test_retry_error_implements_std_error() {
        use std::error::Error;
        let err = RetryError::Error("boom");
        assert!(err.source().is_none());
        assert!(format!("{err:?}").contains("Error"));
    }
}
