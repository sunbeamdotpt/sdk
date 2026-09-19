#![cfg(all(feature = "g2v-server", feature = "testing", feature = "g2v-sqlx"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)

//! Integration tests for health checks.
//!
//! By default these tests start a Postgres container via `testcontainers`. Set
//! `DATABASE_URL` to run against an existing Postgres instead.
//!
//! ```text
//! cargo test -p sunbeam-g2v --test health_integration -- --nocapture --test-threads=1
//! ```

use sdk::g2v::health::{HealthCheck, HealthCheckResult, HealthRouter};
use sdk::g2v::testing::TestHarness;
use std::sync::Arc;

#[path = "g2v_support/mod.rs"]
mod support;

// ============================================================================
// Probe helper
// ============================================================================

async fn probe_pg() -> Option<sqlx::PgPool> {
    match support::containers::postgres_pool().await {
        Ok(pool) => Some(pool),
        Err(e) => {
            eprintln!(
                "[health_integration] Postgres not reachable: {e}; skipping. \
                 Set DATABASE_URL or ensure Docker is available to enable these tests."
            );
            None
        }
    }
}

// ============================================================================
// Hand-rolled failing check (used in the 503 harness test)
// ============================================================================

struct FailingCheck;

impl HealthCheck for FailingCheck {
    fn name(&self) -> &str {
        "failing"
    }

    fn check(
        &self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = sdk::g2v::error::ServiceResult<HealthCheckResult>>
                + Send
                + 'static,
        >,
    > {
        Box::pin(async { Ok(HealthCheckResult::unhealthy("injected failure")) })
    }
}

// ============================================================================
// 1. DatabaseHealthCheck against real Postgres
// ============================================================================

#[tokio::test]
async fn test_database_health_check_against_workspace_postgres() {
    let Some(pool) = probe_pg().await else {
        return;
    };

    let check = sdk::g2v::health::DatabaseHealthCheck::new(pool);
    let result = check.check().await.expect("check returned Err");

    assert!(
        result.is_healthy(),
        "DatabaseHealthCheck should be healthy against real postgres; detail: {:?}",
        result.details
    );
}

// ============================================================================
// 2. HealthRouter /ready 200 when DB passes
// ============================================================================

#[tokio::test]
async fn test_health_router_ready_200_when_db_passes() {
    let Some(pool) = probe_pg().await else {
        return;
    };

    let health_router = HealthRouter::new()
        .with_check(Arc::new(sdk::g2v::health::DatabaseHealthCheck::new(pool)))
        .into_axum_router();

    let mut harness = TestHarness::new().with_routes(health_router);
    harness.start().await.unwrap();

    let resp = harness.get("/health/ready").await.unwrap();
    assert_eq!(
        resp.status,
        200,
        "/health/ready should return 200 when all checks pass; body: {}",
        resp.body_text()
    );

    let body: serde_json::Value =
        serde_json::from_slice(&resp.body).expect("response should be valid JSON");
    assert_eq!(body["status"], "ok", "status field should be 'ok'");
    assert!(body["checks"].is_array(), "checks should be an array");

    let checks = body["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 1, "should have 1 check entry");
    assert_eq!(
        checks[0]["healthy"], true,
        "database check should be healthy"
    );

    harness.stop().await.unwrap();
}

// ============================================================================
// 3. HealthRouter /ready 503 when one check fails (no real service needed)
// ============================================================================

#[tokio::test]
async fn test_health_router_ready_503_when_one_fails() {
    // This test doesn't need real services — it uses the hand-rolled FailingCheck.
    struct PassingCheck;
    impl HealthCheck for PassingCheck {
        fn name(&self) -> &str {
            "passing"
        }
        fn check(
            &self,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = sdk::g2v::error::ServiceResult<HealthCheckResult>>
                    + Send
                    + 'static,
            >,
        > {
            Box::pin(async { Ok(HealthCheckResult::healthy()) })
        }
    }

    let health_router = HealthRouter::new()
        .with_check(Arc::new(PassingCheck))
        .with_check(Arc::new(FailingCheck))
        .into_axum_router();

    let mut harness = TestHarness::new().with_routes(health_router);
    harness.start().await.unwrap();

    let resp = harness.get("/health/ready").await.unwrap();
    assert_eq!(
        resp.status,
        503,
        "/health/ready should return 503 when a check fails; body: {}",
        resp.body_text()
    );

    let body: serde_json::Value =
        serde_json::from_slice(&resp.body).expect("response should be valid JSON");
    assert_eq!(body["status"], "degraded", "status should be 'degraded'");

    let checks = body["checks"].as_array().unwrap();
    let failing = checks
        .iter()
        .find(|e| e["name"] == "failing")
        .expect("should have a 'failing' entry");
    assert_eq!(failing["healthy"], false);
    assert_eq!(failing["detail"], "injected failure");

    harness.stop().await.unwrap();
}
