//! Axum router that exposes liveness and readiness health endpoints.
//!
//! - `GET /health/live`  — always 200 `{"status":"ok"}`.
//! - `GET /health/ready` — 200 if all registered checks pass, 503 otherwise.
//!
//! Build with [`HealthRouter`]:
//!
//! ```rust,no_run
//! # use std::sync::Arc;
//! # use crate::g2v::health::{HealthRouter, HttpHealthCheck};
//! let router = HealthRouter::new()
//!     .with_check(Arc::new(HttpHealthCheck::with_url("upstream", "http://localhost:9999/ping")))
//!     .into_axum_router();
//! ```

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use futures::future::join_all;
use serde::Serialize;
use std::sync::Arc;

use super::{HealthCheck, HealthCheckResult};

// ============================================================================
// JSON shapes
// ============================================================================

#[derive(Debug, Serialize)]
struct CheckEntry {
    name: String,
    healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
struct ReadinessResponse {
    status: &'static str,
    checks: Vec<CheckEntry>,
}

#[derive(Debug, Serialize)]
struct LivenessResponse {
    status: &'static str,
}

// ============================================================================
// HealthRouter
// ============================================================================

/// Builder that accumulates health checks and produces an axum [`Router`].
pub struct HealthRouter {
    /// Registered checks (exposed so `mod.rs` tests can inspect the length).
    pub(super) checks: Vec<Arc<dyn HealthCheck>>,
}

impl HealthRouter {
    /// Create an empty router with no checks registered.
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// Register a check. Returns `self` for chaining.
    pub fn with_check(mut self, check: Arc<dyn HealthCheck>) -> Self {
        self.checks.push(check);
        self
    }

    /// Consume the builder and return an axum [`Router`] with:
    /// - `GET /health/live`
    /// - `GET /health/ready`
    pub fn into_axum_router(self) -> Router {
        let state = Arc::new(self.checks);
        Router::new()
            .route("/health/live", get(live_handler))
            .route(
                "/health/ready",
                get(ready_handler).with_state(Arc::clone(&state)),
            )
    }
}

impl Default for HealthRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// `GET /health/live` — always returns 200.
async fn live_handler() -> (StatusCode, Json<LivenessResponse>) {
    (StatusCode::OK, Json(LivenessResponse { status: "ok" }))
}

/// `GET /health/ready` — runs all registered checks in parallel.
async fn ready_handler(
    State(checks): State<Arc<Vec<Arc<dyn HealthCheck>>>>,
) -> (StatusCode, Json<ReadinessResponse>) {
    // Fire all checks concurrently.
    let futures: Vec<_> = checks
        .iter()
        .map(|c| {
            let name = c.name().to_string();
            let fut = c.check();
            async move { (name, fut.await) }
        })
        .collect();

    let results = join_all(futures).await;

    let mut entries = Vec::with_capacity(results.len());
    let mut all_healthy = true;

    for (name, outcome) in results {
        let check_result: HealthCheckResult = match outcome {
            Ok(r) => r,
            Err(e) => HealthCheckResult::unhealthy(format!("internal error: {e}")),
        };
        if !check_result.is_healthy() {
            all_healthy = false;
        }
        entries.push(CheckEntry {
            name,
            healthy: check_result.healthy,
            detail: check_result.details,
        });
    }

    let status_code = if all_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let body = ReadinessResponse {
        status: if all_healthy { "ok" } else { "degraded" },
        checks: entries,
    };

    (status_code, Json(body))
}
