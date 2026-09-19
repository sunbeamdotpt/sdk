//! OpenTelemetry telemetry bootstrap — tracer provider, OTLP export, and
//! `tracing` subscriber integration.
//!
//! Call [`init`] once at service startup. It installs a W3C
//! `TraceContextPropagator`, builds an [`SdkTracerProvider`] (with a batch
//! OTLP/HTTP exporter when an endpoint is configured), and registers a
//! `tracing-subscriber` registry that bridges `tracing` spans into
//! OpenTelemetry. The returned [`TelemetryGuard`] flushes and shuts down the
//! provider on drop.
//!
//! Without an OTLP endpoint, spans are still created and trace context is
//! still propagated — they are simply not exported.
//!
//! ```rust,no_run
//! let _guard = crate::g2v::telemetry::init(Default::default())?;
//! # Ok::<(), crate::g2v::error::ServiceError>(())
//! ```

use std::sync::atomic::{AtomicBool, Ordering};

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig as _;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{SdkTracerProvider, Tracer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::g2v::error::{ServiceError, ServiceResult};

/// Telemetry owns process-global state, so only one initialization can succeed.
static TELEMETRY_INITIALIZED: AtomicBool = AtomicBool::new(false);

fn claim_initialization() -> ServiceResult<()> {
    TELEMETRY_INITIALIZED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| ())
        .map_err(|_| ServiceError::Configuration("telemetry already initialized".to_string()))
}

fn release_initialization() {
    TELEMETRY_INITIALIZED.store(false, Ordering::Release);
}

/// Default OTLP traces path appended to a bare collector endpoint.
const OTLP_TRACES_PATH: &str = "/v1/traces";

/// Telemetry configuration.
///
/// `Default` reads the standard OpenTelemetry environment variables:
/// `OTEL_SERVICE_NAME`, `OTEL_EXPORTER_OTLP_ENDPOINT`, and
/// `OTEL_TRACES_SAMPLER_ARG` (a `0.0..=1.0` ratio).
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Service name reported as the OTel resource attribute.
    pub service_name: String,
    /// Optional service version reported alongside the name.
    pub service_version: Option<String>,
    /// OTLP/HTTP traces endpoint (e.g. `http://otelcol:4318/v1/traces`).
    ///
    /// When a bare collector base URL is given (from the environment or
    /// directly), `/v1/traces` is appended automatically. When `None`, spans
    /// are created and context is propagated but nothing is exported.
    pub otlp_endpoint: Option<String>,
    /// Fraction of root spans to sample (`0.0..=1.0`). Parent-based: spans
    /// with a sampled remote parent are always kept. Defaults to `1.0`.
    pub sample_ratio: f64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
        let sample_ratio = match std::env::var("OTEL_TRACES_SAMPLER_ARG") {
            Ok(v) => match v.parse::<f64>() {
                Ok(ratio) => ratio,
                Err(_) => {
                    tracing::warn!("invalid OTEL_TRACES_SAMPLER_ARG {v:?}; defaulting to 1.0");
                    1.0
                }
            },
            Err(_) => 1.0,
        };
        let service_name = match std::env::var("OTEL_SERVICE_NAME") {
            Ok(name) => name,
            Err(_) => "sunbeam-service".to_string(),
        };
        Self {
            service_name,
            service_version: None,
            otlp_endpoint,
            sample_ratio,
        }
    }
}

impl TelemetryConfig {
    /// Normalize the configured endpoint into a full OTLP traces URL.
    fn traces_endpoint(&self) -> Option<String> {
        self.otlp_endpoint.as_ref().map(|endpoint| {
            let trimmed = endpoint.trim_end_matches('/');
            if trimmed.ends_with(OTLP_TRACES_PATH) {
                trimmed.to_string()
            } else {
                format!("{trimmed}{OTLP_TRACES_PATH}")
            }
        })
    }
}

/// Guard that flushes pending spans and shuts down the tracer provider on
/// drop. Keep it alive for the lifetime of the service.
pub struct TelemetryGuard {
    provider: SdkTracerProvider,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Err(e) = self.provider.shutdown() {
            eprintln!("sunbeam-g2v telemetry: failed to flush tracer provider: {e}");
        }
    }
}

/// Build a tracer and its provider without touching process-global state.
fn build_tracer(config: &TelemetryConfig) -> ServiceResult<(SdkTracerProvider, Tracer)> {
    let mut resource_builder =
        opentelemetry_sdk::Resource::builder().with_service_name(config.service_name.clone());
    if let Some(version) = &config.service_version {
        resource_builder = resource_builder.with_attribute(opentelemetry::KeyValue::new(
            "service.version",
            version.clone(),
        ));
    }

    let mut provider_builder = SdkTracerProvider::builder()
        .with_resource(resource_builder.build())
        .with_sampler(opentelemetry_sdk::trace::Sampler::ParentBased(Box::new(
            opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(
                config.sample_ratio.clamp(0.0, 1.0),
            ),
        )));

    if let Some(endpoint) = config.traces_endpoint() {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .build()
            .map_err(|e| ServiceError::Configuration(format!("invalid OTLP endpoint: {e}")))?;
        provider_builder = provider_builder.with_batch_exporter(exporter);
    } else {
        tracing::debug!(msg = "no OTLP endpoint configured; spans are recorded but not exported");
    }

    let provider = provider_builder.build();
    let tracer = provider.tracer("sunbeam-g2v");
    Ok((provider, tracer))
}

fn install_globals(provider: &SdkTracerProvider) {
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
    opentelemetry::global::set_tracer_provider(provider.clone());
}

/// Build a tracer (and its provider) for the given configuration, install the
/// W3C trace-context propagator, and set the global tracer provider.
///
/// Use this when you want to compose your own `tracing` subscriber; most
/// services should call [`init`] instead. The returned tracer is already
/// registered globally.
///
/// This consumes the process-global telemetry initialization slot. Call either
/// [`tracer`] or [`init`] once, not both.
pub fn tracer(config: &TelemetryConfig) -> ServiceResult<(SdkTracerProvider, Tracer)> {
    claim_initialization()?;
    let (provider, tracer) = match build_tracer(config) {
        Ok(result) => result,
        Err(error) => {
            release_initialization();
            return Err(error);
        }
    };
    install_globals(&provider);
    Ok((provider, tracer))
}

/// Initialize OpenTelemetry tracing and install the global `tracing`
/// subscriber.
///
/// The subscriber combines an `EnvFilter` (from `RUST_LOG`, defaulting to
/// `info`), a `fmt` layer (with the `logging` feature), and the
/// OpenTelemetry layer that turns `tracing` spans into OTel spans.
///
/// Fails if telemetry has already been initialized or a global subscriber has
/// already been installed. A failed initialization leaves existing globals
/// untouched.
pub fn init(config: TelemetryConfig) -> ServiceResult<TelemetryGuard> {
    claim_initialization()?;
    let (provider, tracer) = match build_tracer(&config) {
        Ok(result) => result,
        Err(error) => {
            release_initialization();
            return Err(error);
        }
    };

    let filter = match tracing_subscriber::EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => tracing_subscriber::EnvFilter::new("info"),
    };

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_opentelemetry::layer().with_tracer(tracer));

    #[cfg(feature = "g2v-server")]
    let registry = registry.with(tracing_subscriber::fmt::layer());

    if let Err(error) = registry.try_init() {
        if let Err(shutdown_error) = provider.shutdown() {
            tracing::warn!("failed to shut down uninstalled telemetry provider: {shutdown_error}");
        }
        release_initialization();
        return Err(ServiceError::Configuration(format!(
            "tracing subscriber init: {error}"
        )));
    }

    install_globals(&provider);
    Ok(TelemetryGuard { provider })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_traces_endpoint_appends_traces_path() {
        let config = TelemetryConfig {
            otlp_endpoint: Some("http://localhost:4318".to_string()),
            ..Default::default()
        };
        assert_eq!(
            config.traces_endpoint().as_deref(),
            Some("http://localhost:4318/v1/traces")
        );
    }

    #[test]
    fn test_traces_endpoint_keeps_full_path() {
        let config = TelemetryConfig {
            otlp_endpoint: Some("http://localhost:4318/v1/traces/".to_string()),
            ..Default::default()
        };
        assert_eq!(
            config.traces_endpoint().as_deref(),
            Some("http://localhost:4318/v1/traces")
        );
    }

    #[test]
    fn test_traces_endpoint_none_when_unset() {
        let config = TelemetryConfig {
            otlp_endpoint: None,
            ..Default::default()
        };
        assert!(config.traces_endpoint().is_none());
    }

    #[test]
    fn test_tracer_without_exporter_builds() {
        let config = TelemetryConfig {
            service_name: "test-service".to_string(),
            otlp_endpoint: None,
            ..Default::default()
        };
        let (provider, _tracer) = tracer(&config).expect("tracer builds without exporter");
        provider.shutdown().expect("shutdown succeeds");
    }
}
