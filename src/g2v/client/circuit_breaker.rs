//! Circuit breaker pattern for service clients.
//!
//! Implements the classic three-state circuit breaker as a Tower [`Layer`]:
//!
//! - **Closed** — normal operation; all requests forwarded to the inner service.
//! - **Open** — fast-fail; requests are rejected immediately with a
//!   `ServiceError::Unavailable` response without calling the inner service.
//! - **HalfOpen** — probe mode; one request is admitted.  On success the circuit
//!   closes; on failure it reopens.
//!
//! # Configuration
//!
//! Use [`CircuitBreakerConfig`] to tune the thresholds.  The defaults are:
//! - `failure_threshold`: 5 consecutive failures opens the circuit.
//! - `success_threshold`: 2 consecutive successes in half-open closes it.
//! - `open_duration`: 30 seconds.
//! - `half_open_max_calls`: 1 (one probe at a time).
//!
//! # Failure predicate
//!
//! A "failure" is any `Err` from the inner service **or** any response for which
//! the configurable predicate returns `true` (default: 5xx status codes).
//!
//! # Example
//!
//! ```rust,no_run
//! use crate::g2v::client::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerLayer};
//! use std::sync::Arc;
//!
//! let config = CircuitBreakerConfig::default();
//! let breaker = Arc::new(CircuitBreaker::new(config));
//! let layer = CircuitBreakerLayer::new(Arc::clone(&breaker));
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tower::{Layer, Service};

/// Predicate that decides whether a response counts as a failure.
pub type FailurePredicate = Arc<dyn Fn(&http::Response<bytes::Bytes>) -> bool + Send + Sync>;

// ============================================================================
// State
// ============================================================================

/// Internal circuit breaker state (full detail).
#[derive(Debug)]
struct CircuitState {
    phase: Phase,
    /// Consecutive failures in Closed state.
    consecutive_failures: u32,
    /// Consecutive successes in HalfOpen state.
    consecutive_successes: u32,
    /// How many calls are currently in-flight in HalfOpen.
    half_open_in_flight: u32,
    /// When the circuit was opened (set when transitioning to Open).
    opened_at: Option<Instant>,
}

/// The phase the circuit is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Normal operation — all requests are forwarded to the inner service.
    Closed,
    /// Fast-fail — requests are rejected immediately without calling the inner service.
    Open,
    /// Probe mode — a limited number of requests are admitted to test recovery.
    HalfOpen,
}

impl CircuitState {
    fn new() -> Self {
        Self {
            phase: Phase::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
            half_open_in_flight: 0,
            opened_at: None,
        }
    }
}

// ============================================================================
// Config
// ============================================================================

/// Configuration for [`CircuitBreaker`].
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit.
    pub failure_threshold: u32,
    /// Number of consecutive successes in half-open required to close.
    pub success_threshold: u32,
    /// How long to stay open before moving to half-open.
    pub open_duration: Duration,
    /// Maximum concurrent probe calls allowed in half-open.
    pub half_open_max_calls: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            open_duration: Duration::from_secs(30),
            half_open_max_calls: 1,
        }
    }
}

// ============================================================================
// CircuitBreaker
// ============================================================================

/// State machine circuit breaker.
///
/// Cheap to clone — all state is behind an `Arc<Mutex<_>>`.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: Arc<Mutex<CircuitState>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    /// Create a circuit breaker with the given configuration.
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(CircuitState::new())),
            config,
        }
    }

    /// Create a circuit breaker with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    /// Return the current phase without modifying state.
    pub async fn phase(&self) -> Phase {
        let state = self.state.lock().await;
        self.effective_phase(&state)
    }

    /// Decide the effective phase, advancing Open→HalfOpen when the window has
    /// expired.  This is a read-only check; the actual transition happens in
    /// [`try_acquire`].
    fn effective_phase(&self, state: &CircuitState) -> Phase {
        match state.phase {
            Phase::Open => {
                if let Some(opened_at) = state.opened_at
                    && opened_at.elapsed() >= self.config.open_duration
                {
                    return Phase::HalfOpen;
                }
                Phase::Open
            }
            other => other,
        }
    }

    /// Try to acquire a "slot" to make a call.
    ///
    /// Returns:
    /// - `Ok(true)` — call is allowed; caller must call [`Self::record_success`] or
    ///   [`Self::record_failure`] when done.
    /// - `Ok(false)` — circuit is open; do not call inner service.
    pub async fn try_acquire(&self) -> bool {
        let mut state = self.state.lock().await;
        match self.effective_phase(&state) {
            Phase::Closed => true,
            Phase::Open => false,
            Phase::HalfOpen => {
                // Materialise the transition if we haven't yet.
                if state.phase == Phase::Open {
                    state.phase = Phase::HalfOpen;
                    state.consecutive_failures = 0;
                    state.consecutive_successes = 0;
                    state.half_open_in_flight = 0;
                }
                if state.half_open_in_flight < self.config.half_open_max_calls {
                    state.half_open_in_flight += 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Record a successful call.
    pub async fn record_success(&self) {
        let mut state = self.state.lock().await;
        match state.phase {
            Phase::Closed => {
                state.consecutive_failures = 0;
            }
            Phase::HalfOpen => {
                state.half_open_in_flight = state.half_open_in_flight.saturating_sub(1);
                state.consecutive_successes += 1;
                if state.consecutive_successes >= self.config.success_threshold {
                    state.phase = Phase::Closed;
                    state.consecutive_failures = 0;
                    state.consecutive_successes = 0;
                    state.opened_at = None;
                }
            }
            Phase::Open => {
                // Shouldn't happen in normal flow, ignore.
            }
        }
    }

    /// Record a failed call.
    pub async fn record_failure(&self) {
        let mut state = self.state.lock().await;
        match state.phase {
            Phase::Closed => {
                state.consecutive_failures += 1;
                if state.consecutive_failures >= self.config.failure_threshold {
                    state.phase = Phase::Open;
                    state.opened_at = Some(Instant::now());
                    state.consecutive_successes = 0;
                }
            }
            Phase::HalfOpen => {
                state.half_open_in_flight = state.half_open_in_flight.saturating_sub(1);
                // Any failure in half-open reopens.
                state.phase = Phase::Open;
                state.opened_at = Some(Instant::now());
                state.consecutive_failures = 0;
                state.consecutive_successes = 0;
            }
            Phase::Open => {
                // Still open; refresh the opened_at timestamp.
                state.opened_at = Some(Instant::now());
            }
        }
    }

    /// Force-reset the circuit to Closed.
    pub async fn force_reset(&self) {
        let mut state = self.state.lock().await;
        *state = CircuitState::new();
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ============================================================================
// Default failure predicate
// ============================================================================

fn default_is_failure<B>(resp: &http::Response<B>) -> bool {
    resp.status().as_u16() >= 500
}

// ============================================================================
// CircuitBreakerLayer
// ============================================================================

/// Tower [`Layer`] that wraps a service with circuit-breaker logic.
#[derive(Clone)]
pub struct CircuitBreakerLayer {
    breaker: Arc<CircuitBreaker>,
    is_failure: FailurePredicate,
}

impl std::fmt::Debug for CircuitBreakerLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreakerLayer")
            .field("breaker", &self.breaker)
            .finish()
    }
}

impl CircuitBreakerLayer {
    /// Create a layer using the given shared breaker.
    pub fn new(breaker: Arc<CircuitBreaker>) -> Self {
        Self {
            breaker,
            is_failure: Arc::new(default_is_failure),
        }
    }

    /// Override the failure predicate.
    pub fn with_failure_predicate<F>(mut self, f: F) -> Self
    where
        F: Fn(&http::Response<bytes::Bytes>) -> bool + Send + Sync + 'static,
    {
        self.is_failure = Arc::new(f) as FailurePredicate;
        self
    }
}

impl<S> Layer<S> for CircuitBreakerLayer {
    type Service = CircuitBreakerService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CircuitBreakerService {
            inner,
            breaker: Arc::clone(&self.breaker),
            is_failure: Arc::clone(&self.is_failure),
        }
    }
}

// ============================================================================
// CircuitBreakerService
// ============================================================================

/// Tower [`Service`] produced by [`CircuitBreakerLayer`].
#[derive(Clone)]
pub struct CircuitBreakerService<S> {
    inner: S,
    breaker: Arc<CircuitBreaker>,
    is_failure: FailurePredicate,
}

impl<S, B> Service<http::Request<B>> for CircuitBreakerService<S>
where
    S: Service<http::Request<B>> + Clone + Send + 'static,
    S::Response: Into<http::Response<bytes::Bytes>> + Send + 'static,
    S::Error: Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = http::Response<bytes::Bytes>;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        let breaker = Arc::clone(&self.breaker);
        let is_failure = Arc::clone(&self.is_failure);

        // Clone inner; swap so self.inner stays poll_ready-d (same pattern as
        // RetryService).
        let inner = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, inner);

        Box::pin(async move {
            // Check whether this call is allowed.
            let allowed = breaker.try_acquire().await;

            if !allowed {
                // Circuit is open — fast-fail with 503.
                let resp = match http::Response::builder()
                    .status(http::StatusCode::SERVICE_UNAVAILABLE)
                    .header(http::header::CONTENT_TYPE, "text/plain")
                    .header(crate::g2v::client::retry::CIRCUIT_OPEN_HEADER, "true")
                    .body(bytes::Bytes::from_static(
                        b"Service Unavailable (circuit open)",
                    )) {
                    Ok(resp) => resp,
                    Err(e) => {
                        tracing::warn!("failed to build circuit-open response: {e}");
                        let mut resp = http::Response::new(bytes::Bytes::from_static(
                            b"Service Unavailable (circuit open)",
                        ));
                        *resp.status_mut() = http::StatusCode::SERVICE_UNAVAILABLE;
                        resp.headers_mut().insert(
                            crate::g2v::client::retry::CIRCUIT_OPEN_HEADER,
                            http::HeaderValue::from_static("true"),
                        );
                        resp
                    }
                };
                return Ok(resp);
            }

            // Call inner.
            match inner.call(req).await {
                Err(e) => {
                    breaker.record_failure().await;
                    Err(e)
                }
                Ok(resp) => {
                    let resp: http::Response<bytes::Bytes> = resp.into();
                    if (is_failure)(&resp) {
                        breaker.record_failure().await;
                    } else {
                        breaker.record_success().await;
                    }
                    Ok(resp)
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

    fn fast_config(failures: u32) -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold: failures,
            success_threshold: 2,
            open_duration: Duration::from_millis(50),
            half_open_max_calls: 1,
        }
    }

    // -------------------------------------------------------------------------
    // State-machine unit tests against CircuitBreaker directly
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_breaker_starts_closed() {
        let cb = CircuitBreaker::with_defaults();
        assert_eq!(cb.phase().await, Phase::Closed);
    }

    #[tokio::test]
    async fn test_breaker_opens_after_threshold_failures() {
        let cb = CircuitBreaker::new(fast_config(3));

        for _ in 0..2 {
            cb.record_failure().await;
            assert_eq!(cb.phase().await, Phase::Closed);
        }

        cb.record_failure().await;
        assert_eq!(cb.phase().await, Phase::Open);
    }

    #[tokio::test]
    async fn test_breaker_open_rejects_503_without_calling_inner() {
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

        let cb = Arc::new(CircuitBreaker::new(fast_config(1)));
        // Trip the breaker.
        cb.record_failure().await;
        assert_eq!(cb.phase().await, Phase::Open);

        let layer = CircuitBreakerLayer::new(Arc::clone(&cb));
        let mut svc = ServiceBuilder::new().layer(layer).service(inner);

        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(http::Request::builder().body(String::new()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), http::StatusCode::SERVICE_UNAVAILABLE);
        // Inner was never called.
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_breaker_open_to_half_open_after_duration() {
        tokio::time::pause();

        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            open_duration: Duration::from_millis(100),
            half_open_max_calls: 1,
        });

        cb.record_failure().await;
        assert_eq!(cb.phase().await, Phase::Open);

        // Advance time past open_duration.
        tokio::time::advance(Duration::from_millis(110)).await;

        assert_eq!(cb.phase().await, Phase::HalfOpen);
    }

    #[tokio::test]
    async fn test_breaker_half_open_on_success_closes() {
        tokio::time::pause();

        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            open_duration: Duration::from_millis(100),
            half_open_max_calls: 2,
        });

        // Open.
        cb.record_failure().await;
        assert_eq!(cb.phase().await, Phase::Open);

        // Advance past open window.
        tokio::time::advance(Duration::from_millis(110)).await;
        assert_eq!(cb.phase().await, Phase::HalfOpen);

        // Acquire and succeed twice.
        assert!(cb.try_acquire().await);
        cb.record_success().await;
        assert_eq!(cb.phase().await, Phase::HalfOpen); // need one more

        assert!(cb.try_acquire().await);
        cb.record_success().await;
        assert_eq!(cb.phase().await, Phase::Closed);
    }

    #[tokio::test]
    async fn test_breaker_half_open_on_failure_reopens() {
        tokio::time::pause();

        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            open_duration: Duration::from_millis(100),
            half_open_max_calls: 1,
        });

        // Open.
        cb.record_failure().await;

        // Advance past open window.
        tokio::time::advance(Duration::from_millis(110)).await;
        assert_eq!(cb.phase().await, Phase::HalfOpen);

        // Probe fails → reopen.
        assert!(cb.try_acquire().await);
        cb.record_failure().await;
        assert_eq!(cb.phase().await, Phase::Open);
    }

    // -------------------------------------------------------------------------
    // Tower layer-level tests (service composition)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_breaker_layer_closed_forwards_request() {
        let inner = tower::service_fn(|_req: http::Request<String>| async {
            Ok::<_, std::convert::Infallible>(
                http::Response::builder()
                    .status(200)
                    .body(bytes::Bytes::from_static(b"ok"))
                    .unwrap(),
            )
        });

        let cb = Arc::new(CircuitBreaker::with_defaults());
        let layer = CircuitBreakerLayer::new(Arc::clone(&cb));
        let mut svc = ServiceBuilder::new().layer(layer).service(inner);

        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(http::Request::builder().body(String::new()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn test_breaker_layer_5xx_trips_breaker() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);

        let inner = tower::service_fn(move |_req: http::Request<String>| {
            cc.fetch_add(1, Ordering::SeqCst);
            async {
                Ok::<_, std::convert::Infallible>(
                    http::Response::builder()
                        .status(500)
                        .body(bytes::Bytes::new())
                        .unwrap(),
                )
            }
        });

        let cb = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        }));
        let layer = CircuitBreakerLayer::new(Arc::clone(&cb));
        let mut svc = ServiceBuilder::new().layer(layer).service(inner);

        // Trip: 3 failures.
        for _ in 0..3 {
            let _ = svc
                .ready()
                .await
                .unwrap()
                .call(http::Request::builder().body(String::new()).unwrap())
                .await;
        }

        assert_eq!(cb.phase().await, Phase::Open);

        // Next call should be fast-failed with 503 without hitting inner.
        let n_before = call_count.load(Ordering::SeqCst);
        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(http::Request::builder().body(String::new()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(call_count.load(Ordering::SeqCst), n_before); // inner not called
    }

    #[tokio::test]
    async fn test_breaker_force_reset() {
        let cb = CircuitBreaker::new(fast_config(1));
        cb.record_failure().await;
        assert_eq!(cb.phase().await, Phase::Open);
        cb.force_reset().await;
        assert_eq!(cb.phase().await, Phase::Closed);
    }

    #[tokio::test]
    async fn test_breaker_record_success_in_closed_resets_failures() {
        let cb = CircuitBreaker::new(fast_config(2));
        cb.record_failure().await;
        cb.record_failure().await;
        // Two consecutive failures should trip.
        assert_eq!(cb.phase().await, Phase::Open);

        cb.force_reset().await;
        cb.record_failure().await;
        cb.record_success().await;
        cb.record_failure().await;
        // First failure was reset by success, so threshold not reached.
        assert_eq!(cb.phase().await, Phase::Closed);
    }

    #[tokio::test]
    async fn test_breaker_half_open_max_calls_limits_probes() {
        tokio::time::pause();
        let cb = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            open_duration: Duration::from_millis(50),
            half_open_max_calls: 1,
        }));

        cb.record_failure().await;
        assert_eq!(cb.phase().await, Phase::Open);

        tokio::time::advance(Duration::from_millis(60)).await;
        // First acquire succeeds.
        assert!(cb.try_acquire().await);
        // Second acquire is denied because max in-flight is reached.
        assert!(!cb.try_acquire().await);
    }

    #[tokio::test]
    async fn test_breaker_custom_failure_predicate() {
        let inner = tower::service_fn(|_req: http::Request<String>| async {
            Ok::<_, std::convert::Infallible>(
                http::Response::builder()
                    .status(418)
                    .body(bytes::Bytes::new())
                    .unwrap(),
            )
        });

        let cb = Arc::new(CircuitBreaker::new(fast_config(1)));
        let layer = CircuitBreakerLayer::new(Arc::clone(&cb))
            .with_failure_predicate(|resp| resp.status() == http::StatusCode::IM_A_TEAPOT);
        let mut svc = ServiceBuilder::new().layer(layer).service(inner);

        let _ = svc
            .ready()
            .await
            .unwrap()
            .call(http::Request::builder().body(String::new()).unwrap())
            .await;
        assert_eq!(cb.phase().await, Phase::Open);
    }

    #[tokio::test]
    async fn test_breaker_layer_records_inner_error_as_failure() {
        let inner = tower::service_fn(|_req: http::Request<String>| async {
            Err::<http::Response<bytes::Bytes>, _>("boom")
        });

        let cb = Arc::new(CircuitBreaker::new(fast_config(1)));
        let layer = CircuitBreakerLayer::new(Arc::clone(&cb));
        let mut svc = ServiceBuilder::new().layer(layer).service(inner);

        let _ = svc
            .ready()
            .await
            .unwrap()
            .call(http::Request::builder().body(String::new()).unwrap())
            .await;
        assert_eq!(cb.phase().await, Phase::Open);
    }

    #[test]
    fn test_circuit_breaker_default() {
        let cb = CircuitBreaker::default();
        assert_eq!(cb.config.failure_threshold, 5);
    }

    #[test]
    fn test_circuit_breaker_config_default() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.success_threshold, 2);
        assert_eq!(config.open_duration, Duration::from_secs(30));
    }

    #[test]
    fn test_circuit_breaker_layer_debug() {
        let cb = Arc::new(CircuitBreaker::default());
        let layer = CircuitBreakerLayer::new(cb);
        assert!(format!("{layer:?}").contains("CircuitBreakerLayer"));
    }
}
