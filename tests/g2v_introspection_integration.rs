#![cfg(feature = "g2v-server")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)

//! Integration tests for OAuth 2.0 token introspection as the session
//! validation mechanism, including the G2V-001 failure-class matrix:
//! inactive / gateway error / timeout / transport must surface as distinct
//! classes in the 401 body (and ConnectRPC routes must get Connect-shaped
//! error bodies).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    extract::Request,
    http::StatusCode,
    middleware,
    response::{IntoResponse, Json},
    routing::get,
};
use sdk::g2v::middleware::auth::{
    AuthContext, AuthMiddlewareState, TenantId, auth_middleware,
    introspection::{IntrospectionConfig, IntrospectionSessionClient},
};
use serde_json::json;
use tokio::net::TcpListener;

const VALID_TOKEN: &str = "valid-access-token";
const INACTIVE_TOKEN: &str = "inactive-access-token";
const VALID_SUBJECT: &str = "user-42";
const VALID_TENANT: &str = "01ARYZ6S41TSV4RRFFQ69G5FAV";

// ============================================================================
// Mock introspection server
//
// Behavior switches on the path so each failure class can be triggered:
// - /oauth2/introspect       — normal (active/inactive by token)
// - /oauth2/introspect_500   — gateway error
// - /oauth2/introspect_slow  — hangs past the client timeout
// ============================================================================

async fn introspection_handler(
    axum::extract::Path(path): axum::extract::Path<String>,
    axum::extract::Form(form): axum::extract::Form<HashMap<String, String>>,
) -> impl IntoResponse {
    match path.as_str() {
        "introspect_500" => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": "upstream exploded" })),
        )
            .into_response(),
        "introspect_slow" => {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Json(json!({ "active": false })).into_response()
        }
        _ => {
            let token = form.get("token").cloned().unwrap_or_default();
            if token == VALID_TOKEN {
                Json(json!({
                    "active": true,
                    "sub": VALID_SUBJECT,
                    "tenant_id": VALID_TENANT,
                    "scope": "openid profile",
                    "client_id": "test-client"
                }))
                .into_response()
            } else {
                Json(json!({ "active": false })).into_response()
            }
        }
    }
}

async fn start_mock_introspection_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock introspection server should bind");
    let addr = listener.local_addr().expect("local addr should exist");

    let app = Router::new().route(
        "/oauth2/{action}",
        axum::routing::post(introspection_handler),
    );

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("mock introspection server should run");
    });

    format!("http://{addr}")
}

// ============================================================================
// Protected app under test
// ============================================================================

async fn whoami_handler(request: Request) -> impl IntoResponse {
    let tenant = request.extensions().get::<TenantId>().cloned();
    let ctx = request.extensions().get::<AuthContext>().cloned();

    match (tenant, ctx) {
        (Some(tenant), Some(ctx)) if ctx.is_authenticated() => Json(json!({
            "tenant": tenant.0,
            "subject": ctx.subject,
        }))
        .into_response(),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthenticated" })),
        )
            .into_response(),
    }
}

async fn start_introspection_app(config: IntrospectionConfig) -> String {
    let state = AuthMiddlewareState::new(Arc::new(IntrospectionSessionClient::new(config)));

    let app = Router::new()
        .route("/whoami", get(whoami_handler))
        // A ConnectRPC-shaped route to exercise the Connect error body.
        .route(
            "/connectrpc.eliza.v1.ElizaService/Say",
            axum::routing::post(|| async { Json(json!({ "sentence": "hi" })) }),
        )
        .layer(middleware::from_fn_with_state(state, auth_middleware));

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test app should bind");
    let addr = listener.local_addr().expect("local addr should exist");

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("test app should run");
    });

    format!("http://{addr}")
}

fn test_config(base: &str, action: &str) -> IntrospectionConfig {
    IntrospectionConfig {
        url: format!("{base}/oauth2/{action}"),
        client_id: "test-client".to_string(),
        client_secret: "test-secret".to_string(),
        timeout: None,
    }
}

/// Extract the failure class from either body shape.
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
async fn introspection_valid_token_resolves_subject_and_tenant() {
    let base = start_mock_introspection_server().await;
    let app_url = start_introspection_app(test_config(&base, "introspect")).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{app_url}/whoami"))
        .header("x-session-token", VALID_TOKEN)
        .send()
        .await
        .expect("request should complete");

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.expect("response should be json");
    assert_eq!(body["tenant"], VALID_TENANT);
    assert_eq!(body["subject"], VALID_SUBJECT);
}

#[tokio::test]
async fn introspection_bearer_token_also_validates() {
    let base = start_mock_introspection_server().await;
    let app_url = start_introspection_app(test_config(&base, "introspect")).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{app_url}/whoami"))
        .bearer_auth(VALID_TOKEN)
        .send()
        .await
        .expect("request should complete");

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn introspection_inactive_token_returns_401_with_inactive_class() {
    let base = start_mock_introspection_server().await;
    let app_url = start_introspection_app(test_config(&base, "introspect")).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{app_url}/whoami"))
        .header("x-session-token", INACTIVE_TOKEN)
        .send()
        .await
        .expect("request should complete");

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = resp.json().await.expect("response should be json");
    assert_eq!(failure_class(&body), "inactive");
}

#[tokio::test]
async fn introspection_gateway_error_returns_401_with_gateway_class() {
    let base = start_mock_introspection_server().await;
    let app_url = start_introspection_app(test_config(&base, "introspect_500")).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{app_url}/whoami"))
        .bearer_auth(VALID_TOKEN)
        .send()
        .await
        .expect("request should complete");

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = resp.json().await.expect("response should be json");
    assert_eq!(failure_class(&body), "gateway_error");
}

#[tokio::test]
async fn introspection_timeout_returns_401_with_timeout_class() {
    let base = start_mock_introspection_server().await;
    let config = test_config(&base, "introspect_slow").with_timeout(Duration::from_millis(150));
    let app_url = start_introspection_app(config).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{app_url}/whoami"))
        .bearer_auth(VALID_TOKEN)
        .send()
        .await
        .expect("request should complete");

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = resp.json().await.expect("response should be json");
    assert_eq!(failure_class(&body), "timeout");
}

#[tokio::test]
async fn introspection_unreachable_returns_401_with_transport_class() {
    // Bind then drop a listener so the port is guaranteed closed.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("temp listener should bind");
    let dead_addr = listener.local_addr().expect("local addr");
    drop(listener);

    let config = test_config(&format!("http://{dead_addr}"), "introspect")
        .with_timeout(Duration::from_millis(500));
    let app_url = start_introspection_app(config).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{app_url}/whoami"))
        .bearer_auth(VALID_TOKEN)
        .send()
        .await
        .expect("request should complete");

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = resp.json().await.expect("response should be json");
    assert_eq!(failure_class(&body), "transport");
}

#[tokio::test]
async fn connectrpc_route_gets_connect_shaped_error() {
    let base = start_mock_introspection_server().await;
    let app_url = start_introspection_app(test_config(&base, "introspect")).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{app_url}/connectrpc.eliza.v1.ElizaService/Say"))
        .header("content-type", "application/json")
        .bearer_auth(INACTIVE_TOKEN)
        .body(r#"{"sentence":"hello"}"#)
        .send()
        .await
        .expect("request should complete");

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = resp.json().await.expect("response should be json");
    assert_eq!(body["code"], "unauthenticated");
    assert_eq!(body["message"], "inactive");
}

#[tokio::test]
async fn introspection_missing_token_returns_401_with_missing_credentials_class() {
    let base = start_mock_introspection_server().await;
    let app_url = start_introspection_app(test_config(&base, "introspect")).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{app_url}/whoami"))
        .send()
        .await
        .expect("request should complete");

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = resp.json().await.expect("response should be json");
    assert_eq!(failure_class(&body), "missing_credentials");
}
