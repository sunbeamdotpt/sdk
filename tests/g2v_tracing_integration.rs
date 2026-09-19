#![cfg(all(feature = "g2v-server", feature = "g2v-server"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)

//! Integration tests for request-id and tracing middleware defaults.
//!
//! These tests boot a real server via [`TestHarness`] (no containers needed):
//!
//! ```text
//! cargo test -p sunbeam-g2v --test tracing_integration -- --nocapture --test-threads=1
//! ```

use std::collections::HashMap;

use axum::{Router as AxumRouter, extract::Request, routing::get};
use sdk::g2v::middleware::request_id::RequestId;
use sdk::g2v::testing::TestHarness;

/// Handler that reports the request id seen in request extensions.
async fn request_id_handler(req: Request) -> String {
    RequestId::extract(&req).unwrap_or_else(|| "missing".to_string())
}

async fn start_harness() -> TestHarness {
    let routes = AxumRouter::new().route("/ping", get(request_id_handler));
    let mut harness = TestHarness::new().with_routes(routes);
    harness.start().await.expect("harness starts");
    harness
}

#[tokio::test]
async fn test_request_id_generated_and_echoed() {
    let mut harness = start_harness().await;

    let resp = harness.get("/ping").await.expect("request succeeds");
    assert!(resp.is_success());

    let echoed = resp
        .get_header("x-request-id")
        .expect("response carries x-request-id");
    assert!(
        uuid::Uuid::parse_str(echoed).is_ok(),
        "generated id is a uuid, got {echoed}"
    );
    assert_eq!(
        resp.body_text(),
        *echoed,
        "handler saw the same id via request extensions"
    );

    harness.stop().await.expect("harness stops");
}

#[tokio::test]
async fn test_client_supplied_request_id_honored() {
    let mut harness = start_harness().await;

    let mut headers = HashMap::new();
    headers.insert(
        "x-request-id".to_string(),
        "client-supplied-123".to_string(),
    );
    let resp = harness
        .request("GET", "/ping", None, Some(headers))
        .await
        .expect("request succeeds");

    assert_eq!(
        resp.get_header("x-request-id").map(String::as_str),
        Some("client-supplied-123"),
        "server echoes the client-supplied id"
    );
    assert_eq!(resp.body_text(), "client-supplied-123");

    harness.stop().await.expect("harness stops");
}

#[tokio::test]
async fn test_traceparent_accepted() {
    let mut harness = start_harness().await;

    let mut headers = HashMap::new();
    headers.insert(
        "traceparent".to_string(),
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string(),
    );
    let resp = harness
        .request("GET", "/ping", None, Some(headers))
        .await
        .expect("request succeeds");

    assert!(resp.is_success());
    assert!(
        resp.get_header("x-request-id").is_some(),
        "request id still assigned alongside trace context"
    );

    harness.stop().await.expect("harness stops");
}
