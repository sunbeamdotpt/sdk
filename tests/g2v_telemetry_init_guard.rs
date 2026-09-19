#![cfg(feature = "g2v-server")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Regression test for duplicate telemetry initialization.

use sdk::g2v::error::ServiceError;
use sdk::g2v::telemetry::{self, TelemetryConfig};

fn config(service_name: &str) -> TelemetryConfig {
    TelemetryConfig {
        service_name: service_name.to_string(),
        service_version: None,
        otlp_endpoint: None,
        sample_ratio: 1.0,
    }
}

#[test]
fn second_telemetry_init_is_rejected() {
    let guard = telemetry::init(config("telemetry-first")).expect("first telemetry init");

    let Err(error) = telemetry::init(config("telemetry-second")) else {
        panic!("second telemetry init must fail");
    };
    assert!(
        matches!(&error, ServiceError::Configuration(message) if message.contains("already initialized")),
        "unexpected error: {error}"
    );

    drop(guard);
}
