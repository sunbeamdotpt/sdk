use std::time::{Duration, SystemTime, UNIX_EPOCH};

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt, core::ContainerPort, runners::AsyncRunner,
};

use crate::testing::util;

/// Minimal Headscale configuration for integration tests.
const CONFIG: &str = r#"server_url: http://127.0.0.1:8080
listen_addr: 0.0.0.0:8080
metrics_listen_addr: 0.0.0.0:9090
grpc_listen_addr: 127.0.0.1:50443
grpc_allow_insecure: false

noise:
  private_key_path: /var/lib/headscale/noise_private.key

prefixes:
  v4: 100.64.0.0/10
  v6: fd7a:115c:a1e0::/48
  allocation: sequential

derp:
  server:
    enabled: false
  urls:
    - https://controlplane.tailscale.com/derpmap/default

dns:
  magic_dns: false
  override_local_dns: false

database:
  type: sqlite
  sqlite:
    path: /var/lib/headscale/db.sqlite
"#;

/// Returns a unique tag so parallel test runs don't race building the same image tag.
fn unique_tag() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}")
}

/// Testcontainers builder for Headscale.
///
/// Defaults to the `headscale/headscale:0.28.0` image used by `../sbbb`.
#[derive(Debug, Clone)]
pub struct Headscale {
    tag: String,
    config: String,
    built_tag: String,
    published_ports: bool,
}

impl Headscale {
    /// Container image name.
    pub const NAME: &'static str = "headscale/headscale";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "0.28.0";

    /// HTTP API port.
    pub const HTTP_PORT: u16 = 8080;
    /// Metrics / debug port.
    pub const METRICS_PORT: u16 = 9090;

    /// Create a new Headscale builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Replace the embedded `config.yaml` configuration.
    pub fn with_config(mut self, config: impl Into<String>) -> Self {
        self.config = config.into();
        self
    }

    /// Publish Headscale's ports to random host ports so the container is reachable
    /// without bridge-network access.
    pub fn publish_ports(mut self) -> Self {
        self.published_ports = true;
        self
    }

    /// Return the HTTP API URL for a container that was started with published ports.
    pub async fn url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::HTTP_PORT).await
    }

    /// Build a small derived image containing the config and start a container from it.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let image_name = "sunbeam-test/headscale";
        let descriptor = format!("{image_name}:{}", self.built_tag);

        let dockerfile = format!(
            "FROM {}:{}\nCOPY config.yaml /etc/headscale/config.yaml\nCMD [\"serve\"]\n",
            Self::NAME,
            self.tag
        );

        util::build_image(
            &descriptor,
            &dockerfile,
            &[("config.yaml", self.config.as_bytes())],
        )
        .await
        .map_err(|e| {
            testcontainers::TestcontainersError::other(format!("build headscale image: {e}"))
        })?;

        let mut image = GenericImage::new(image_name, &self.built_tag)
            .with_exposed_port(ContainerPort::Tcp(Self::HTTP_PORT))
            .with_exposed_port(ContainerPort::Tcp(Self::METRICS_PORT))
            .with_startup_timeout(Duration::from_secs(120));

        if self.published_ports {
            image = image
                .with_mapped_port(0, ContainerPort::Tcp(Self::HTTP_PORT))
                .with_mapped_port(0, ContainerPort::Tcp(Self::METRICS_PORT));
        }

        image.start().await
    }
}

impl Default for Headscale {
    fn default() -> Self {
        Self {
            tag: Self::DEFAULT_TAG.to_owned(),
            config: CONFIG.to_owned(),
            built_tag: unique_tag(),
            published_ports: false,
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use std::time::Duration;

    use super::Headscale;

    #[tokio::test]
    async fn headscale_is_healthy() {
        let container = Headscale::default()
            .publish_ports()
            .start()
            .await
            .expect("headscale should start");

        let base_url = Headscale::url(&container)
            .await
            .expect("headscale url should resolve");

        let url = format!("{base_url}/health");
        let mut last_status = None;
        for _ in 0..30 {
            match reqwest::get(&url).await {
                Ok(resp) if resp.status().is_success() => {
                    last_status = Some(resp.status().to_string());
                    break;
                }
                other => last_status = other.map(|r| r.status().to_string()).ok(),
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        let status = last_status.expect("headscale /health should respond");
        assert!(
            status.starts_with('2'),
            "headscale /health should return 2xx, got {status}"
        );
    }
}
