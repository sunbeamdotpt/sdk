#![cfg(all(all(feature = "g2v-server", feature = "g2v-server"), feature = "testing", feature = "g2v-sqlx"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)

//! End-to-end telemetry test: spans created by the server's `TracingLayer`
//! are exported over OTLP/HTTP to a real OpenTelemetry collector started via
//! the sdk's testcontainer builder.
//!
//! ```text
//! cargo test -p sunbeam-g2v --test telemetry_integration -- --nocapture --test-threads=1
//! ```

use std::collections::HashMap;
use std::time::Duration;

use axum::{Router as AxumRouter, routing::get};
use sdk::g2v::telemetry::{self, TelemetryConfig};
use sdk::g2v::testing::TestHarness;

#[path = "g2v_support/mod.rs"]
mod support;

/// Single test in this binary: `telemetry::init` installs process-global
/// state (subscriber + propagator + provider), so it can only run once.
#[tokio::test]
async fn spans_reach_otlp_collector() {
    support::containers::init_docker_host();

    let collector = match sdk::testing::OtelCollector::new()
        .publish_ports()
        .start()
        .await
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "[telemetry_integration] OTel collector not available: {e}; skipping. \
                 Ensure Docker is available to enable this test."
            );
            return;
        }
    };
    let endpoint = sdk::testing::OtelCollector::endpoint(&collector)
        .await
        .expect("collector endpoint resolves");

    let guard = telemetry::init(TelemetryConfig {
        service_name: "g2v-telemetry-test".to_string(),
        service_version: Some("0.0.0-test".to_string()),
        otlp_endpoint: Some(endpoint),
        sample_ratio: 1.0,
    })
    .expect("telemetry init");

    let routes = AxumRouter::new().route("/ping", get(|| async { "pong" }));
    let mut harness = TestHarness::new().with_routes(routes);
    harness.start().await.expect("harness starts");

    let mut headers = HashMap::new();
    headers.insert(
        "x-request-id".to_string(),
        "telemetry-e2e-request-id".to_string(),
    );
    headers.insert(
        "traceparent".to_string(),
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string(),
    );
    let resp = harness
        .request("GET", "/ping", None, Some(headers))
        .await
        .expect("request succeeds");
    assert!(resp.is_success());
    assert_eq!(
        resp.get_header("x-request-id").map(String::as_str),
        Some("telemetry-e2e-request-id")
    );
    harness.stop().await.expect("harness stops");

    // Dropping the guard shuts down the provider, flushing the batch exporter.
    drop(guard);

    // The collector's debug exporter (verbosity: detailed) logs every span.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let logs = collector
            .stderr_to_vec()
            .await
            .expect("collector logs readable");
        let logs = String::from_utf8_lossy(&logs);
        if logs.contains("GET /ping") && logs.contains("telemetry-e2e-request-id") {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "collector did not log the server span; logs:\n{logs}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
