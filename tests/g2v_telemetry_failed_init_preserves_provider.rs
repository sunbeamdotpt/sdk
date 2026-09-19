#![cfg(feature = "g2v-server")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Regression test: a failed telemetry initialization must not replace an
//! already-installed global tracer provider.

use std::sync::{Arc, Mutex};

use opentelemetry::trace::Tracer as _;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{SdkTracerProvider, SpanData, SpanExporter};
use sdk::g2v::error::ServiceError;
use sdk::g2v::telemetry::{self, TelemetryConfig};

#[derive(Clone, Debug, Default)]
struct RecordingExporter {
    spans: Arc<Mutex<Vec<String>>>,
}

impl SpanExporter for RecordingExporter {
    fn export(
        &self,
        batch: Vec<SpanData>,
    ) -> impl std::future::Future<Output = OTelSdkResult> + Send {
        let spans = Arc::clone(&self.spans);
        async move {
            let mut guard = spans.lock().expect("recording exporter lock");
            guard.extend(batch.into_iter().map(|span| span.name.into_owned()));
            Ok(())
        }
    }
}

#[test]
fn failed_init_preserves_existing_global_provider() {
    let exporter = RecordingExporter::default();
    let recorded_spans = Arc::clone(&exporter.spans);
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter)
        .build();
    opentelemetry::global::set_tracer_provider(provider.clone());
    tracing_subscriber::fmt()
        .try_init()
        .expect("plain subscriber installs");

    let Err(error) = telemetry::init(TelemetryConfig {
        service_name: "telemetry-failed-init".to_string(),
        service_version: None,
        otlp_endpoint: None,
        sample_ratio: 1.0,
    }) else {
        panic!("telemetry init must fail when a subscriber already exists");
    };
    assert!(
        matches!(&error, ServiceError::Configuration(message) if message.contains("tracing subscriber init")),
        "unexpected error: {error}"
    );

    let tracer = opentelemetry::global::tracer("telemetry-init-guard-probe");
    let span = tracer.start("original-provider-span");
    drop(span);
    provider.force_flush().expect("provider flushes");

    assert!(
        recorded_spans
            .lock()
            .expect("recorded spans lock")
            .iter()
            .any(|name| name == "original-provider-span"),
        "failed init replaced the existing global provider"
    );
    provider.shutdown().expect("provider shuts down");
}
