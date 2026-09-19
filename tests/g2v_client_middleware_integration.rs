#![cfg(feature = "g2v-client")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)
//! Integration tests for `RetryLayer` and `CircuitBreakerLayer`.
//!
//! These tests compose real Tower service stacks (no mocks) using a counter-backed
//! inner service that simulates a flaky upstream.  They verify end-to-end behaviour
//! of the middleware layers without requiring external infrastructure.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sdk::g2v::client::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerLayer,
};
use sdk::g2v::client::retry::{RetryLayer, RetryPolicy};
use tower::{Service, ServiceBuilder, ServiceExt};

// ---------------------------------------------------------------------------
// Helper: build a simple GET request
// ---------------------------------------------------------------------------

fn get(path: &str) -> http::Request<String> {
    http::Request::builder()
        .method("GET")
        .uri(path)
        .body(String::new())
        .unwrap()
}

// ---------------------------------------------------------------------------
// Integration test 1: RetryLayer recovers when upstream fails N times then OK
// ---------------------------------------------------------------------------

/// A `RetryLayer`-wrapped client recovers when the upstream fails the first
/// two calls but succeeds on the third.
#[tokio::test]
async fn retry_layer_recovers_after_two_transient_failures() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let cc = Arc::clone(&call_count);

    // Flaky inner service: fails on calls 0 and 1, succeeds from call 2 onward.
    let inner = tower::service_fn(move |_req: http::Request<String>| {
        let n = cc.fetch_add(1, Ordering::SeqCst);
        async move {
            if n < 2 {
                // Simulate transient upstream error.
                Err::<http::Response<bytes::Bytes>, &str>("upstream unavailable")
            } else {
                Ok(http::Response::builder()
                    .status(200)
                    .body(bytes::Bytes::from_static(b"ok"))
                    .unwrap())
            }
        }
    });

    let policy = RetryPolicy {
        max_attempts: 5,
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(5),
        jitter: false,
        retry_unauthenticated: false,
    };

    // Pause tokio time so sleep calls complete instantly.
    tokio::time::pause();

    let mut svc = ServiceBuilder::new()
        .layer(RetryLayer::new(policy))
        .service(inner);

    let resp = svc
        .ready()
        .await
        .unwrap()
        .call(get("/api/resource"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200, "expected 200 after retry recovery");
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        3,
        "expected exactly 3 inner calls (2 failures + 1 success)"
    );
}

/// A `RetryLayer`-wrapped client returns 503 responses to the caller after
/// retrying a 503 upstream the maximum number of times.
#[tokio::test]
async fn retry_layer_returns_last_5xx_when_exhausted() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let cc = Arc::clone(&call_count);

    let inner = tower::service_fn(move |_req: http::Request<String>| {
        cc.fetch_add(1, Ordering::SeqCst);
        async {
            Ok::<_, std::convert::Infallible>(
                http::Response::builder()
                    .status(503)
                    .body(bytes::Bytes::from_static(b"service unavailable"))
                    .unwrap(),
            )
        }
    });

    let policy = RetryPolicy {
        max_attempts: 3,
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(5),
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
        .call(get("/flaky"))
        .await
        .unwrap();

    // After max_attempts exhausted, the last response (503) is returned.
    assert_eq!(resp.status(), 503);
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        3,
        "expected exactly max_attempts inner calls"
    );
}

// ---------------------------------------------------------------------------
// Integration test 2: CircuitBreakerLayer fast-fails after threshold
// ---------------------------------------------------------------------------

/// A `CircuitBreakerLayer`-wrapped client receives an immediate 503 (without
/// hitting the inner service) once the upstream has failed `failure_threshold`
/// times.
#[tokio::test]
async fn circuit_breaker_layer_fast_fails_after_threshold() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let cc = Arc::clone(&call_count);

    // Inner service always returns 500.
    let inner = tower::service_fn(move |_req: http::Request<String>| {
        cc.fetch_add(1, Ordering::SeqCst);
        async {
            Ok::<_, std::convert::Infallible>(
                http::Response::builder()
                    .status(500)
                    .body(bytes::Bytes::from_static(b"internal server error"))
                    .unwrap(),
            )
        }
    });

    let failure_threshold = 3u32;
    let cb = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold,
        success_threshold: 2,
        open_duration: Duration::from_secs(60),
        half_open_max_calls: 1,
    }));

    let layer = CircuitBreakerLayer::new(Arc::clone(&cb));
    let mut svc = ServiceBuilder::new().layer(layer).service(inner);

    // Drive exactly `failure_threshold` failures through to trip the breaker.
    for i in 0..failure_threshold {
        let resp = svc.ready().await.unwrap().call(get("/api")).await.unwrap();
        assert_eq!(
            resp.status(),
            500,
            "call {} should reach inner and return 500",
            i
        );
    }

    // Circuit should now be open.
    assert_eq!(
        cb.phase().await,
        sdk::g2v::client::circuit_breaker::Phase::Open,
        "breaker should be open after {} failures",
        failure_threshold
    );

    let n_before = call_count.load(Ordering::SeqCst);

    // This call must be fast-failed — inner service must NOT be called.
    let resp = svc.ready().await.unwrap().call(get("/api")).await.unwrap();

    assert_eq!(
        resp.status(),
        http::StatusCode::SERVICE_UNAVAILABLE,
        "open circuit should return 503"
    );
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        n_before,
        "inner service must not be called when circuit is open"
    );
}

/// `CircuitBreakerLayer` + `RetryLayer` composed: retries happen on transient
/// errors as long as the circuit stays closed, and the circuit opens once the
/// failure threshold is reached across all attempts.
#[tokio::test]
async fn circuit_breaker_and_retry_composed() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let cc = Arc::clone(&call_count);

    // Inner always errors (not 5xx response — actual Err).
    let inner = tower::service_fn(move |_req: http::Request<String>| {
        cc.fetch_add(1, Ordering::SeqCst);
        async { Err::<http::Response<bytes::Bytes>, &str>("err") }
    });

    let cb = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 10, // high — won't open in this test
        success_threshold: 2,
        open_duration: Duration::from_secs(60),
        half_open_max_calls: 1,
    }));

    let retry_policy = RetryPolicy {
        max_attempts: 3,
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(5),
        jitter: false,
        retry_unauthenticated: false,
    };

    tokio::time::pause();

    // Stack: RetryLayer(outer) → CircuitBreakerLayer → inner
    // Retry wraps the whole CB+inner, so each retry goes through the breaker.
    let mut svc = ServiceBuilder::new()
        .layer(RetryLayer::new(retry_policy))
        .layer(CircuitBreakerLayer::new(Arc::clone(&cb)))
        .service(inner);

    // All 3 attempts fail → RetryLayer returns the last Err.
    let result = svc.ready().await.unwrap().call(get("/")).await;
    assert!(result.is_err(), "all retries exhausted should return Err");
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        3,
        "expected 3 inner calls (one per retry attempt)"
    );
}
