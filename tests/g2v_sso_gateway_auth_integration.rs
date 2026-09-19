#![cfg(all(feature = "g2v-server", feature = "testing", feature = "g2v-sqlx"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)

//! End-to-end auth middleware tests against a real sso-gateway deployment.
//!
//! The gateway stack (Postgres + Hydra + Kratos + OpenFGA + gateway image) is
//! started by the sdk's [`SsoGateway`](sdk::testing::SsoGateway) testcontainer
//! orchestrator; the permission backend is OpenFGA (the sdk default). No Ory
//! client is used directly: tokens are minted through the gateway's public
//! OAuth2 endpoints and validated through its public `/oauth2/introspect`,
//! exactly like a production g2v service would.
//!
//! ```text
//! cargo test -p sunbeam-g2v --test sso_gateway_auth_integration -- --nocapture --test-threads=1
//! ```

use std::sync::Arc;

use axum::{
    Router as AxumRouter,
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

#[path = "g2v_support/mod.rs"]
mod support;

/// Bootstrap credentials: the sdk orchestrator derives a random secret per
/// stack unless overridden; pin it so the test can mint tokens.
const BOOTSTRAP_CLIENT_ID: &str = "system-bootstrap-client";
const BOOTSTRAP_CLIENT_SECRET: &str = "g2v-integration-test-bootstrap-secret";

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

/// Start a tiny Axum app protected by `auth_middleware`, validating tokens
/// through the gateway's public introspection endpoint.
async fn start_auth_app(gateway_endpoint: &str) -> String {
    let state = AuthMiddlewareState::new(Arc::new(IntrospectionSessionClient::new(
        IntrospectionConfig {
            url: format!("{gateway_endpoint}/oauth2/introspect"),
            client_id: BOOTSTRAP_CLIENT_ID.to_string(),
            client_secret: BOOTSTRAP_CLIENT_SECRET.to_string(),
            timeout: None,
        },
    )));

    let app = AxumRouter::new()
        .route("/whoami", get(whoami_handler))
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

/// Mint an access token from the gateway's public OAuth2 token endpoint
/// using the pinned bootstrap client credentials.
async fn mint_bootstrap_token(gateway_endpoint: &str, scope: &str) -> Option<String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{gateway_endpoint}/oauth2/token"))
        .basic_auth(BOOTSTRAP_CLIENT_ID, Some(BOOTSTRAP_CLIENT_SECRET))
        .form(&[("grant_type", "client_credentials"), ("scope", scope)])
        .send()
        .await
        .expect("token request should complete");
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.expect("token response is json");
    body.get("access_token")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

#[tokio::test]
async fn sso_gateway_end_to_end_authn() {
    support::containers::init_docker_host();

    let gateway = match sdk::testing::SsoGateway::new()
        .with_env("SYSTEM_BOOTSTRAP_CLIENT_SECRET", BOOTSTRAP_CLIENT_SECRET)
        .start()
        .await
    {
        Ok(gateway) => gateway,
        Err(e) => {
            eprintln!(
                "[sso_gateway_auth_integration] sso-gateway stack not available: {e}; skipping. \
                 Ensure Docker is available to enable this test."
            );
            return;
        }
    };
    let endpoint = gateway.endpoint().to_string();
    let app_url = start_auth_app(&endpoint).await;
    let client = reqwest::Client::new();

    // 1. A minted token validates through the gateway and authenticates.
    let Some(token) = mint_bootstrap_token(&endpoint, "tenant:admin").await else {
        gateway.shutdown().await;
        panic!("gateway should mint a bootstrap token");
    };

    let resp = client
        .get(format!("{app_url}/whoami"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("request should complete");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.expect("response should be json");
    assert!(
        body["tenant"].as_str().is_some_and(|t| !t.is_empty()),
        "tenant resolved: {body}"
    );

    // 2. A garbage token is rejected with the `inactive` failure class.
    let resp = client
        .get(format!("{app_url}/whoami"))
        .bearer_auth("not-a-real-token")
        .send()
        .await
        .expect("request should complete");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = resp.json().await.expect("rejection is json");
    let class = body
        .get("error")
        .or_else(|| body.get("message"))
        .and_then(|v| v.as_str())
        .expect("failure class present");
    assert_eq!(class, "inactive", "body: {body}");

    gateway.shutdown().await;
}
