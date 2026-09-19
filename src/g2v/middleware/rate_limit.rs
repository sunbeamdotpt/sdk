//! Token-bucket rate limiter middleware.
//!
//! Provides a simple in-memory rate limiter and an Axum middleware that
//! rejects requests with `429 Too Many Requests` when the bucket is empty.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Simple token-bucket rate limiter.
#[derive(Clone, Debug)]
pub struct RateLimiter {
    max: u32,
    per: Duration,
    state: Arc<Mutex<RateLimiterState>>,
}

#[derive(Clone, Debug)]
struct RateLimiterState {
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    /// Create a limiter that allows `max` requests per `per` duration.
    pub fn new(max: u32, per: Duration) -> Self {
        Self {
            max,
            per,
            state: Arc::new(Mutex::new(RateLimiterState {
                tokens: max as f64,
                last: Instant::now(),
            })),
        }
    }

    /// Attempt to consume one token. Returns `true` if the request is allowed.
    pub fn check(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let elapsed = now.duration_since(state.last).as_secs_f64();
        let refill = elapsed * (self.max as f64 / self.per.as_secs_f64());
        state.tokens = (state.tokens + refill).min(self.max as f64);
        state.last = now;
        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Reject requests with `429 Too Many Requests` when the rate limiter is empty.
pub async fn rate_limit_middleware(
    axum::extract::State(limiter): axum::extract::State<Arc<RateLimiter>>,
    request: Request,
    next: Next,
) -> Response {
    if limiter.check() {
        next.run(request).await
    } else {
        StatusCode::TOO_MANY_REQUESTS.into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_allows_initial_requests() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60));
        assert!(limiter.check());
        assert!(limiter.check());
        assert!(limiter.check());
    }

    #[test]
    fn rate_limiter_rejects_excess() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.check());
        assert!(limiter.check());
        assert!(!limiter.check());
    }

    #[test]
    fn rate_limiter_refills_over_time() {
        let limiter = RateLimiter::new(1, Duration::from_millis(100));
        assert!(limiter.check());
        assert!(!limiter.check());
        std::thread::sleep(Duration::from_millis(150));
        assert!(limiter.check());
    }
}
