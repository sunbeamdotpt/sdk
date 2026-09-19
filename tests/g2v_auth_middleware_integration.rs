#![cfg(feature = "g2v-server")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)

//! Integration tests for the multitenancy-aware auth middleware.
//!
//! These tests run a real `axum` server via `TestHarness` and exercise the
//! `auth_middleware` with in-memory stub session clients/stores. No external
//! identity provider is required; the sso-gateway-backed end-to-end tests
//! live in `sso_gateway_auth_integration.rs`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Router as AxumRouter,
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
};
use serde_json::json;

use sdk::g2v::middleware::auth::error::AuthError;
use sdk::g2v::middleware::auth::session::{SessionClient, SessionStore};
use sdk::g2v::middleware::auth::{
    AuthContext, AuthMiddlewareState, SESSION_COOKIE_NAME, SessionTokenSigner, TenantId,
    auth_middleware,
};
use sdk::g2v::testing::TestHarness;

const VALID_TOKEN: &str = "valid-session-token";
const VALID_TENANT: &str = "01ARYZ6S41TSV4RRFFQ69G5FAV";
const VALID_SUBJECT: &str = "user-42";

// ============================================================================
// Stub stores
// ============================================================================

struct StubSessionClient;

#[async_trait::async_trait]
impl SessionClient for StubSessionClient {
    async fn to_session(
        &self,
        _cookie: Option<&str>,
        token: Option<&str>,
    ) -> Result<serde_json::Value, AuthError> {
        match token {
            Some(VALID_TOKEN) => Ok(json!({
                "tenant_id": VALID_TENANT,
                "sub": VALID_SUBJECT,
            })),
            _ => Err(AuthError::InactiveSession),
        }
    }
}

/// Session store with a fixed active/revoked verdict.
struct StubSessionStore(bool);

#[async_trait::async_trait]
impl SessionStore for StubSessionStore {
    async fn is_active(&self, _sid: &str) -> Result<bool, AuthError> {
        Ok(self.0)
    }
}

fn test_signer() -> SessionTokenSigner {
    SessionTokenSigner::new(
        b"test-cookie-secret-that-is-at-least-32-bytes",
        3600,
        "https://gateway.example.com",
    )
    .expect("test secret is 32+ bytes")
}

fn test_auth_state() -> AuthMiddlewareState {
    AuthMiddlewareState::new(Arc::new(StubSessionClient)).with_session_signer(test_signer())
}

// ============================================================================
// Handlers
// ============================================================================

async fn whoami_handler(request: Request) -> impl IntoResponse {
    let tenant = request.extensions().get::<TenantId>().cloned();
    let ctx = request.extensions().get::<AuthContext>().cloned();

    match (tenant, ctx) {
        (Some(tenant), Some(ctx)) if ctx.is_authenticated() => Json(json!({
            "tenant": tenant.0,
            "subject": ctx.subject,
            "scopes": ctx.scopes,
        }))
        .into_response(),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthenticated" })),
        )
            .into_response(),
    }
}

fn test_app() -> AxumRouter {
    AxumRouter::new()
        .route("/whoami", get(whoami_handler))
        .route(
            "/connectrpc.eliza.v1.ElizaService/Say",
            axum::routing::post(|| async { Json(json!({ "sentence": "hi" })) }),
        )
        .layer(axum::middleware::from_fn_with_state(
            test_auth_state(),
            auth_middleware,
        ))
}

/// Extract the failure class from either rejection body shape.
fn failure_class(body: &serde_json::Value) -> &str {
    body.get("error")
        .or_else(|| body.get("message"))
        .and_then(|v| v.as_str())
        .expect("rejection body carries a failure class")
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn missing_auth_returns_401_with_missing_credentials_class() {
    let mut harness = TestHarness::new().with_routes(test_app());
    harness.start().await.unwrap();

    let resp = harness.get("/whoami").await.unwrap();
    assert_eq!(resp.status, 401, "body: {}", resp.body_text());
    let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(failure_class(&body), "missing_credentials");
    assert!(
        resp.get_header("x-request-id").is_some(),
        "rejections still carry the request id"
    );

    harness.stop().await.unwrap();
}

#[tokio::test]
async fn valid_bearer_token_resolves_tenant_and_subject() {
    let mut harness = TestHarness::new().with_routes(test_app());
    harness.start().await.unwrap();

    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), format!("Bearer {VALID_TOKEN}"));

    let resp = harness
        .request("GET", "/whoami", None, Some(headers))
        .await
        .unwrap();
    assert_eq!(resp.status, 200, "body: {}", resp.body_text());

    let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(body["tenant"], VALID_TENANT);
    assert_eq!(body["subject"], VALID_SUBJECT);

    harness.stop().await.unwrap();
}

#[tokio::test]
async fn invalid_bearer_token_returns_401_with_inactive_class() {
    let mut harness = TestHarness::new().with_routes(test_app());
    harness.start().await.unwrap();

    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        "Bearer not-a-real-token".to_string(),
    );

    let resp = harness
        .request("GET", "/whoami", None, Some(headers))
        .await
        .unwrap();
    assert_eq!(resp.status, 401, "body: {}", resp.body_text());
    let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(failure_class(&body), "inactive");

    harness.stop().await.unwrap();
}

#[tokio::test]
async fn connectrpc_route_rejection_is_connect_shaped() {
    let mut harness = TestHarness::new().with_routes(test_app());
    harness.start().await.unwrap();

    let mut headers = HashMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    headers.insert(
        "authorization".to_string(),
        "Bearer not-a-real-token".to_string(),
    );

    let resp = harness
        .request(
            "POST",
            "/connectrpc.eliza.v1.ElizaService/Say",
            Some(br#"{"sentence":"hello"}"#.to_vec()),
            Some(headers),
        )
        .await
        .unwrap();
    assert_eq!(resp.status, 401, "body: {}", resp.body_text());
    let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(body["code"], "unauthenticated");
    assert_eq!(body["message"], "inactive");

    harness.stop().await.unwrap();
}

#[tokio::test]
async fn signed_session_cookie_authenticates() {
    let (token, _claims) = test_signer()
        .issue(VALID_SUBJECT, VALID_TENANT, "password")
        .expect("token issues");

    let mut harness = TestHarness::new().with_routes(test_app());
    harness.start().await.unwrap();

    let mut headers = HashMap::new();
    headers.insert(
        "cookie".to_string(),
        format!("{SESSION_COOKIE_NAME}={token}"),
    );

    let resp = harness
        .request("GET", "/whoami", None, Some(headers))
        .await
        .unwrap();
    assert_eq!(resp.status, 200, "body: {}", resp.body_text());
    let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(body["tenant"], VALID_TENANT);
    assert_eq!(body["subject"], VALID_SUBJECT);

    harness.stop().await.unwrap();
}

#[tokio::test]
async fn revoked_session_cookie_returns_401_with_inactive_class() {
    let (token, _claims) = test_signer()
        .issue(VALID_SUBJECT, VALID_TENANT, "password")
        .expect("token issues");

    let state = AuthMiddlewareState::new(Arc::new(StubSessionClient))
        .with_session_signer(test_signer())
        .with_session_store(Arc::new(StubSessionStore(false)));
    let app = AxumRouter::new()
        .route("/whoami", get(whoami_handler))
        .layer(axum::middleware::from_fn_with_state(state, auth_middleware));

    let mut harness = TestHarness::new().with_routes(app);
    harness.start().await.unwrap();

    let mut headers = HashMap::new();
    headers.insert(
        "cookie".to_string(),
        format!("{SESSION_COOKIE_NAME}={token}"),
    );

    let resp = harness
        .request("GET", "/whoami", None, Some(headers))
        .await
        .unwrap();
    assert_eq!(resp.status, 401, "body: {}", resp.body_text());
    let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(failure_class(&body), "inactive");

    harness.stop().await.unwrap();
}

#[tokio::test]
async fn tampered_session_cookie_returns_401_with_invalid_session_class() {
    let mut harness = TestHarness::new().with_routes(test_app());
    harness.start().await.unwrap();

    let mut headers = HashMap::new();
    headers.insert(
        "cookie".to_string(),
        format!("{SESSION_COOKIE_NAME}=tampered-token-value"),
    );

    let resp = harness
        .request("GET", "/whoami", None, Some(headers))
        .await
        .unwrap();
    assert_eq!(resp.status, 401, "body: {}", resp.body_text());
    let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(failure_class(&body), "invalid_session");

    harness.stop().await.unwrap();
}

#[tokio::test]
async fn tenant_header_alone_is_not_authentication() {
    let mut harness = TestHarness::new().with_routes(test_app());
    harness.start().await.unwrap();

    let mut headers = HashMap::new();
    headers.insert("x-tenant-id".to_string(), VALID_TENANT.to_string());

    let resp = harness
        .request("GET", "/whoami", None, Some(headers))
        .await
        .unwrap();
    assert_eq!(resp.status, 401, "body: {}", resp.body_text());

    harness.stop().await.unwrap();
}

#[tokio::test]
async fn health_path_is_public_and_skips_middleware() {
    let app = AxumRouter::new()
        .route("/health/live", get(|| async { "ok" }))
        .route("/whoami", get(whoami_handler))
        .layer(axum::middleware::from_fn_with_state(
            test_auth_state(),
            auth_middleware,
        ));

    let mut harness = TestHarness::new().with_routes(app);
    harness.start().await.unwrap();

    let resp = harness.get("/health/live").await.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body_text(), "ok");

    harness.stop().await.unwrap();
}
