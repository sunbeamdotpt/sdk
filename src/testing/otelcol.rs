use std::time::{Duration, SystemTime, UNIX_EPOCH};

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
};

use crate::testing::util;

/// Minimal collector configuration: OTLP/HTTP receiver and a debug exporter
/// with detailed verbosity so received span names appear in the container logs.
const CONFIG: &str = r#"
receivers:
  otlp:
    protocols:
      http:
        endpoint: 0.0.0.0:4318
exporters:
  debug:
    verbosity: detailed
service:
  pipelines:
    traces:
      receivers: [otlp]
      exporters: [debug]
"#;

/// Returns a unique tag so parallel test runs don't race building the same image tag.
fn unique_tag() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}")
}

/// Testcontainers builder for the OpenTelemetry collector.
///
/// Starts a collector with an OTLP/HTTP receiver and a debug exporter that
/// dumps received spans to the container logs (stderr).
#[derive(Debug, Clone)]
pub struct OtelCollector {
    tag: String,
    config: String,
    built_tag: String,
    network: Option<String>,
    container_name: Option<String>,
    published_ports: bool,
}

impl OtelCollector {
    /// Container image name.
    pub const NAME: &'static str = "otel/opentelemetry-collector";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "0.120.0";

    /// OTLP/HTTP receiver port.
    pub const OTLP_HTTP_PORT: u16 = 4318;

    /// Create a new OtelCollector builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Replace the embedded collector configuration.
    pub fn with_config(mut self, config: impl Into<String>) -> Self {
        self.config = config.into();
        self
    }

    /// Publish the OTLP/HTTP port to a random host port so the container is
    /// reachable without bridge-network access.
    pub fn publish_ports(mut self) -> Self {
        self.published_ports = true;
        self
    }

    /// Attach the container to a specific Docker network.
    ///
    /// When no network is set, Docker's default bridge network is used.
    pub fn with_network(mut self, network: impl Into<String>) -> Self {
        self.network = Some(network.into());
        self
    }

    /// Set the Docker container name so other containers can resolve it by name
    /// on the same network.
    pub fn with_container_name(mut self, name: impl Into<String>) -> Self {
        self.container_name = Some(name.into());
        self
    }

    /// Return the OTLP/HTTP endpoint URL for a container that was started with
    /// published ports (`/v1/traces` is appended by the exporter).
    pub async fn endpoint(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::OTLP_HTTP_PORT).await
    }

    /// Build a small derived image containing the config and start a container from it.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let image_name = "sunbeam-test/otelcol";
        let descriptor = format!("{image_name}:{}", self.built_tag);

        let dockerfile = format!(
            "FROM {}:{}\nCOPY otelcol.yaml /etc/otelcol/config.yaml\nCMD [\"--config\", \"/etc/otelcol/config.yaml\"]\n",
            Self::NAME,
            self.tag
        );

        util::build_image(
            &descriptor,
            &dockerfile,
            &[("otelcol.yaml", self.config.as_bytes())],
        )
        .await
        .map_err(|e| {
            testcontainers::TestcontainersError::other(format!("build otelcol image: {e}"))
        })?;

        let mut image = GenericImage::new(image_name, &self.built_tag)
            .with_exposed_port(ContainerPort::Tcp(Self::OTLP_HTTP_PORT))
            .with_wait_for(WaitFor::message_on_either_std(
                "Everything is ready. Begin running and processing data.",
            ))
            .with_startup_timeout(Duration::from_secs(120));

        if let Some(network) = &self.network {
            image = image.with_network(network);
        }

        if let Some(name) = self.container_name {
            image = image.with_container_name(name);
        }

        if self.published_ports {
            image = image.with_mapped_port(0, ContainerPort::Tcp(Self::OTLP_HTTP_PORT));
        }

        image.start().await
    }
}

impl Default for OtelCollector {
    fn default() -> Self {
        Self {
            tag: Self::DEFAULT_TAG.to_owned(),
            config: CONFIG.to_owned(),
            built_tag: unique_tag(),
            network: None,
            container_name: None,
            published_ports: false,
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use std::time::Duration;

    use super::OtelCollector;

    /// Encode a minimal OTLP `ExportTraceServiceRequest` holding a single span by
    /// hand (protobuf wire format), so the test doesn't need an OTLP client.
    fn encode_probe_request(name: &str) -> Vec<u8> {
        fn field(tag: u8, data: &[u8]) -> Vec<u8> {
            let mut v = vec![tag, data.len() as u8];
            v.extend_from_slice(data);
            v
        }

        let trace_id = [1u8; 16];
        let span_id = [2u8; 8];

        // Span: trace_id = 1, span_id = 2, name = 3.
        let mut span = field(0x0A, &trace_id);
        span.extend(field(0x12, &span_id));
        span.extend(field(0x1A, name.as_bytes()));
        // ScopeSpans: spans = 2.
        let scope_spans = field(0x12, &span);
        // ResourceSpans: scope_spans = 2.
        let resource_spans = field(0x12, &scope_spans);
        // ExportTraceServiceRequest: resource_spans = 1.
        field(0x0A, &resource_spans)
    }

    #[tokio::test]
    async fn otelcol_receives_spans_over_otlp_http() {
        let container = OtelCollector::default()
            .publish_ports()
            .start()
            .await
            .expect("otel collector should start");

        let endpoint = OtelCollector::endpoint(&container)
            .await
            .expect("collector endpoint should resolve");
        let addr = endpoint.trim_start_matches("http://").to_string();

        // The published port can take a moment to become reachable (e.g. lima's
        // host-side forwarder); poll until TCP connects succeed.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            match tokio::net::TcpStream::connect(&addr).await {
                Ok(_) => break,
                Err(_) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                Err(e) => panic!("collector port {addr} should be reachable: {e}"),
            }
        }

        let span_name = "sunbeam-test-probe";
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{endpoint}/v1/traces"))
            .header("content-type", "application/x-protobuf")
            .body(encode_probe_request(span_name))
            .send()
            .await
            .expect("OTLP request should send");
        assert!(
            resp.status().is_success(),
            "collector should accept the span, got {}",
            resp.status()
        );

        // The debug exporter (verbosity: detailed) dumps received spans to stderr.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let logs = container
                .stderr_to_vec()
                .await
                .expect("collector logs should be readable");
            let logs = String::from_utf8_lossy(&logs);
            if logs.contains(span_name) {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "collector did not log the probe span; logs:\n{logs}"
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}
