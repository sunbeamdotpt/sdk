//! Health check module for Sunbeam services.
//!
//! Provides liveness and readiness health checks, plus an [`HealthRouter`]
//! that exposes them as axum routes:
//!
//! - `GET /health/live`  — always 200; process is up.
//! - `GET /health/ready` — 200 if all registered checks pass, 503 otherwise.
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(all(feature = "g2v-server", feature = "g2v-sqlx"))]
//! # async fn example(pool: sqlx::PgPool) {
//! use std::sync::Arc;
//! use crate::g2v::health::{DatabaseHealthCheck, HealthRouter};
//!
//! let router = HealthRouter::new()
//!     .with_check(Arc::new(DatabaseHealthCheck::new(pool)))
//!     .into_axum_router();
//! # }
//! ```

pub mod router;

use crate::g2v::error::ServiceResult;
use std::sync::Arc;

pub use router::HealthRouter;

// ============================================================================
// Result type
// ============================================================================

/// Result of a single health check.
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// Whether the component is healthy.
    pub healthy: bool,
    /// Optional human-readable details (required when unhealthy).
    pub details: Option<String>,
}

impl HealthCheckResult {
    /// Create a healthy result.
    pub fn healthy() -> Self {
        Self {
            healthy: true,
            details: None,
        }
    }

    /// Create an unhealthy result with a reason.
    pub fn unhealthy(details: impl Into<String>) -> Self {
        Self {
            healthy: false,
            details: Some(details.into()),
        }
    }

    /// Returns `true` when healthy.
    pub fn is_healthy(&self) -> bool {
        self.healthy
    }
}

// ============================================================================
// Trait
// ============================================================================

/// A single health check that can be polled asynchronously.
///
/// Implementations must be `Send + Sync + 'static` so they can be stored
/// behind `Arc<dyn HealthCheck>` and awaited from any task.
pub trait HealthCheck: Send + Sync + 'static {
    /// Short name used in the readiness JSON response (e.g. `"database"`).
    fn name(&self) -> &str;

    /// Run the check and return a result.
    fn check(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = ServiceResult<HealthCheckResult>> + Send + 'static>,
    >;
}

// ============================================================================
// DatabaseHealthCheck
// ============================================================================

/// Health check that issues `SELECT 1` against a Postgres pool.
///
/// The query is wrapped in a 2-second timeout. Any error (connection failure,
/// timeout, etc.) is returned as an unhealthy result rather than propagating
/// as a `ServiceError`, so the readiness endpoint always returns a structured
/// JSON payload.
#[cfg(feature = "g2v-sqlx")]
pub struct DatabaseHealthCheck {
    pool: sqlx::PgPool,
}

#[cfg(feature = "g2v-sqlx")]
impl DatabaseHealthCheck {
    /// Create a check from a live pool.
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "g2v-sqlx")]
impl HealthCheck for DatabaseHealthCheck {
    fn name(&self) -> &str {
        "database"
    }

    fn check(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = ServiceResult<HealthCheckResult>> + Send + 'static>,
    > {
        let pool = self.pool.clone();
        Box::pin(async move {
            let timeout = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                sqlx::query("SELECT 1").execute(&pool),
            )
            .await;

            match timeout {
                Ok(Ok(_)) => Ok(HealthCheckResult::healthy()),
                Ok(Err(e)) => Ok(HealthCheckResult::unhealthy(format!("query failed: {e}"))),
                Err(_) => Ok(HealthCheckResult::unhealthy("timed out after 2s")),
            }
        })
    }
}

// ============================================================================
// PermissionHealthCheck
// ============================================================================

/// Health check for the authorization backend.
///
/// Issues a permission check against a sentinel namespace/object. Any HTTP
/// response (including "denied") means the backend is reachable. A transport
/// error means it is unreachable.
#[cfg(feature = "g2v-server")]
pub struct PermissionHealthCheck {
    client: crate::g2v::middleware::auth::authorization::AuthorizationClient,
}

#[cfg(feature = "g2v-server")]
impl PermissionHealthCheck {
    /// Create a check from an existing authorization client.
    pub fn new(client: crate::g2v::middleware::auth::authorization::AuthorizationClient) -> Self {
        Self { client }
    }
}

#[cfg(feature = "g2v-server")]
impl HealthCheck for PermissionHealthCheck {
    fn name(&self) -> &str {
        "authorization"
    }

    fn check(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = ServiceResult<HealthCheckResult>> + Send + 'static>,
    > {
        let client = self.client.clone();
        Box::pin(async move {
            // A successful response (allowed OR denied) proves the backend is up.
            // Only a transport/network error is treated as unhealthy.
            let timeout = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                client.check_permission("__health__", "__probe__", "probe", "__health_probe__"),
            )
            .await;

            match timeout {
                // Any response from the backend means it is reachable.
                Ok(Ok(_))
                | Ok(Err(crate::g2v::middleware::auth::error::AuthError::AuthorizationBackend(
                    _,
                ))) => Ok(HealthCheckResult::healthy()),
                Ok(Err(e)) => Ok(HealthCheckResult::unhealthy(format!(
                    "authorization error: {e}"
                ))),
                Err(_) => Ok(HealthCheckResult::unhealthy("timed out after 2s")),
            }
        })
    }
}

// ============================================================================
// NatsHealthCheck
// ============================================================================

/// Health check for a NATS connection.
///
/// Returns healthy if `client.connection_state()` is `Connected`.
#[cfg(feature = "g2v-nats")]
pub struct NatsHealthCheck {
    client: async_nats::Client,
}

#[cfg(feature = "g2v-nats")]
impl NatsHealthCheck {
    /// Create a check from an existing NATS client.
    pub fn new(client: async_nats::Client) -> Self {
        Self { client }
    }
}

#[cfg(feature = "g2v-nats")]
impl HealthCheck for NatsHealthCheck {
    fn name(&self) -> &str {
        "nats"
    }

    fn check(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = ServiceResult<HealthCheckResult>> + Send + 'static>,
    > {
        use async_nats::connection::State;
        let state = self.client.connection_state();
        Box::pin(async move {
            if state == State::Connected {
                Ok(HealthCheckResult::healthy())
            } else {
                Ok(HealthCheckResult::unhealthy(format!(
                    "NATS connection state: {state:?}"
                )))
            }
        })
    }
}

// ============================================================================
// HttpHealthCheck
// ============================================================================

/// Health check that GETs a URL and asserts a 2xx response.
///
/// Uses a 2-second timeout. Useful for poking upstream HTTP services.
pub struct HttpHealthCheck {
    client: reqwest::Client,
    url: String,
    name: String,
}

impl HttpHealthCheck {
    /// Create a check with an explicit name, client, and URL.
    pub fn new(name: impl Into<String>, client: reqwest::Client, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            client,
            url: url.into(),
        }
    }

    /// Convenience constructor that builds a default reqwest client.
    pub fn with_url(name: impl Into<String>, url: impl Into<String>) -> Self {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
        {
            Ok(client) => client,
            Err(e) => {
                // Only fails when the process TLS backend is broken; fall back
                // to the default client and make the misconfiguration visible.
                #[cfg(feature = "g2v-server")]
                tracing::warn!(msg = "failed to build health-check HTTP client; using defaults", error = %e);
                let _ = &e;
                reqwest::Client::new()
            }
        };
        Self::new(name, client, url)
    }
}

impl HealthCheck for HttpHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = ServiceResult<HealthCheckResult>> + Send + 'static>,
    > {
        let client = self.client.clone();
        let url = self.url.clone();
        Box::pin(async move {
            let timeout =
                tokio::time::timeout(std::time::Duration::from_secs(2), client.get(&url).send())
                    .await;

            match timeout {
                Ok(Ok(resp)) if resp.status().is_success() => Ok(HealthCheckResult::healthy()),
                Ok(Ok(resp)) => Ok(HealthCheckResult::unhealthy(format!(
                    "HTTP {} from {url}",
                    resp.status()
                ))),
                Ok(Err(e)) => Ok(HealthCheckResult::unhealthy(format!("request error: {e}"))),
                Err(_) => Ok(HealthCheckResult::unhealthy("timed out after 2s")),
            }
        })
    }
}

// ============================================================================
// CompositeHealthCheck
// ============================================================================

/// Runs a sequence of checks and short-circuits on the first failure.
///
/// All checks are run in order; the first unhealthy result is returned
/// immediately. If all checks pass, a healthy result is returned.
pub struct CompositeHealthCheck {
    checks: Vec<Arc<dyn HealthCheck>>,
}

impl CompositeHealthCheck {
    /// Create an empty composite check.
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// Add a check to the sequence.
    pub fn add_check(mut self, check: Arc<dyn HealthCheck>) -> Self {
        self.checks.push(check);
        self
    }
}

impl Default for CompositeHealthCheck {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CompositeHealthCheck {
    fn clone(&self) -> Self {
        Self {
            checks: self.checks.clone(),
        }
    }
}

impl HealthCheck for CompositeHealthCheck {
    fn name(&self) -> &str {
        "composite"
    }

    fn check(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = ServiceResult<HealthCheckResult>> + Send + 'static>,
    > {
        let checks = self.checks.clone();
        Box::pin(async move {
            for check in &checks {
                let result = check.check().await?;
                if !result.is_healthy() {
                    return Ok(result);
                }
            }
            Ok(HealthCheckResult::healthy())
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

    // -----------------------------------------------------------------------
    // HealthCheckResult
    // -----------------------------------------------------------------------

    #[test]
    fn test_health_check_result_healthy() {
        let result = HealthCheckResult::healthy();
        assert!(result.is_healthy());
        assert!(result.details.is_none());
    }

    #[test]
    fn test_health_check_result_unhealthy() {
        let result = HealthCheckResult::unhealthy("Connection failed");
        assert!(!result.is_healthy());
        assert_eq!(result.details, Some("Connection failed".to_string()));
    }

    // -----------------------------------------------------------------------
    // CompositeHealthCheck
    // -----------------------------------------------------------------------

    struct AlwaysHealthy;
    impl HealthCheck for AlwaysHealthy {
        fn name(&self) -> &str {
            "always-healthy"
        }
        fn check(
            &self,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = ServiceResult<HealthCheckResult>> + Send + 'static,
            >,
        > {
            Box::pin(async { Ok(HealthCheckResult::healthy()) })
        }
    }

    struct AlwaysUnhealthy;
    impl HealthCheck for AlwaysUnhealthy {
        fn name(&self) -> &str {
            "always-unhealthy"
        }
        fn check(
            &self,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = ServiceResult<HealthCheckResult>> + Send + 'static,
            >,
        > {
            Box::pin(async { Ok(HealthCheckResult::unhealthy("injected failure")) })
        }
    }

    #[tokio::test]
    async fn test_composite_all_healthy() {
        let composite = CompositeHealthCheck::new()
            .add_check(Arc::new(AlwaysHealthy))
            .add_check(Arc::new(AlwaysHealthy));
        let result = composite.check().await.unwrap();
        assert!(result.is_healthy());
    }

    #[tokio::test]
    async fn test_composite_short_circuits_on_first_failure() {
        // First check fails; second check is healthy but should never be reached.
        let composite = CompositeHealthCheck::new()
            .add_check(Arc::new(AlwaysUnhealthy))
            .add_check(Arc::new(AlwaysHealthy));
        let result = composite.check().await.unwrap();
        assert!(!result.is_healthy());
        assert_eq!(result.details, Some("injected failure".to_string()));
    }

    #[tokio::test]
    async fn test_composite_empty_is_healthy() {
        let composite = CompositeHealthCheck::new();
        let result = composite.check().await.unwrap();
        assert!(result.is_healthy());
    }

    // -----------------------------------------------------------------------
    // HealthRouter builder
    // -----------------------------------------------------------------------

    #[test]
    fn test_health_router_new_is_empty() {
        let r = HealthRouter::new();
        assert!(r.checks.is_empty());
    }

    #[test]
    fn test_health_router_with_check_adds() {
        let r = HealthRouter::new()
            .with_check(Arc::new(AlwaysHealthy))
            .with_check(Arc::new(AlwaysUnhealthy));
        assert_eq!(r.checks.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Route handler behaviour
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_live_always_200() {
        use crate::g2v::testing::TestHarness;

        let health_router = HealthRouter::new()
            .with_check(Arc::new(AlwaysUnhealthy)) // checks shouldn't matter for /live
            .into_axum_router();

        let mut harness = TestHarness::new().with_routes(health_router);
        harness.start().await.unwrap();

        let resp = harness.get("/health/live").await.unwrap();
        assert_eq!(resp.status, 200, "live must always be 200");
        harness.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_ready_200_when_all_pass() {
        use crate::g2v::testing::TestHarness;

        let health_router = HealthRouter::new()
            .with_check(Arc::new(AlwaysHealthy))
            .with_check(Arc::new(AlwaysHealthy))
            .into_axum_router();

        let mut harness = TestHarness::new().with_routes(health_router);
        harness.start().await.unwrap();

        let resp = harness.get("/health/ready").await.unwrap();
        assert_eq!(resp.status, 200);

        let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(body["status"], "ok");
        assert!(body["checks"].is_array());
        harness.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_ready_503_when_one_fails() {
        use crate::g2v::testing::TestHarness;

        let health_router = HealthRouter::new()
            .with_check(Arc::new(AlwaysHealthy))
            .with_check(Arc::new(AlwaysUnhealthy))
            .into_axum_router();

        let mut harness = TestHarness::new().with_routes(health_router);
        harness.start().await.unwrap();

        let resp = harness.get("/health/ready").await.unwrap();
        assert_eq!(resp.status, 503);

        let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(body["status"], "degraded");
        harness.stop().await.unwrap();
    }

    #[test]
    fn test_composite_default_clone_and_name() {
        let composite = CompositeHealthCheck::default().add_check(Arc::new(AlwaysHealthy));
        let cloned = composite.clone();
        assert_eq!(cloned.name(), "composite");
        assert_eq!(cloned.checks.len(), 1);
    }

    #[cfg(feature = "g2v-sqlx")]
    #[tokio::test]
    async fn test_database_health_check_backend_unavailable() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://g2v:invalid@127.0.0.1:1/g2v")
            .unwrap();
        let check = DatabaseHealthCheck::new(pool);

        assert_eq!(check.name(), "database");
        let result = check.check().await.unwrap();
        assert!(!result.is_healthy());
        let details = result.details.as_deref().unwrap();
        assert!(details.contains("query failed") || details.contains("timed out"));
    }

    #[cfg(feature = "g2v-server")]
    #[tokio::test]
    async fn test_permission_health_check_backend_unavailable() {
        use crate::g2v::middleware::auth::authorization::{
            AuthorizationClient, AuthorizationConfig,
        };

        let client = AuthorizationClient::new(AuthorizationConfig {
            read_url: "http://127.0.0.1:1".to_string(),
            write_url: "http://127.0.0.1:1".to_string(),
        })
        .unwrap();
        let check = PermissionHealthCheck::new(client);

        assert_eq!(check.name(), "authorization");
        let result = check.check().await.unwrap();
        assert!(!result.is_healthy());
        assert!(
            result
                .details
                .as_deref()
                .is_some_and(|details| details.contains("authorization error"))
        );
    }

    #[tokio::test]
    async fn test_http_health_check_success_and_error_status() {
        use axum::{Router, http::StatusCode, routing::get};

        let app = Router::new()
            .route("/ok", get(|| async { StatusCode::OK }))
            .route("/fail", get(|| async { StatusCode::SERVICE_UNAVAILABLE }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let healthy = HttpHealthCheck::with_url("upstream-ok", format!("http://{addr}/ok"));
        assert_eq!(healthy.name(), "upstream-ok");
        assert!(healthy.check().await.unwrap().is_healthy());

        let unhealthy = HttpHealthCheck::new(
            "upstream-fail",
            reqwest::Client::new(),
            format!("http://{addr}/fail"),
        );
        let result = unhealthy.check().await.unwrap();
        assert!(!result.is_healthy());
        assert!(
            result
                .details
                .as_deref()
                .is_some_and(|details| details.contains("HTTP 503"))
        );
    }

    #[tokio::test]
    async fn test_http_health_check_request_error() {
        let check =
            HttpHealthCheck::new("closed-port", reqwest::Client::new(), "http://127.0.0.1:1");
        let result = check.check().await.unwrap();
        assert!(!result.is_healthy());
        assert!(
            result
                .details
                .as_deref()
                .is_some_and(|details| details.contains("request error"))
        );
    }
}
