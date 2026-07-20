use std::time::{Duration, SystemTime, UNIX_EPOCH};

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt, core::ContainerPort, runners::AsyncRunner,
};

use crate::testing::util;

/// Minimal Tuwunel configuration for integration tests.
const CONFIG: &str = r#"[global]
server_name = "localhost"
database_path = "/var/lib/tuwunel"
port = [8008]
address = "0.0.0.0"
allow_registration = true
registration_token = "sunbeam-test-token"
allow_encryption = true
allow_federation = false
log = "info"
"#;

/// Returns a unique tag so parallel test runs don't race building the same image tag.
fn unique_tag() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}")
}

/// Testcontainers builder for Tuwunel.
///
/// Defaults to the public `ghcr.io/matrix-construct/tuwunel:latest` image.
/// `../sbbb` uses a custom `src.DOMAIN_SUFFIX/studio/tuwunel` image; override
/// it with `.with_tag(...)` if you need to match the deployment exactly.
#[derive(Debug, Clone)]
pub struct Tuwunel {
    tag: String,
    config: String,
    built_tag: String,
    published_ports: bool,
}

impl Tuwunel {
    /// Container image name.
    pub const NAME: &'static str = "ghcr.io/matrix-construct/tuwunel";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "latest";

    /// Client / federation port.
    pub const PORT: u16 = 8008;

    /// Create a new Tuwunel builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Replace the embedded `tuwunel.toml` configuration.
    pub fn with_config(mut self, config: impl Into<String>) -> Self {
        self.config = config.into();
        self
    }

    /// Publish Tuwunel's port to a random host port so the container is reachable
    /// without bridge-network access.
    pub fn publish_ports(mut self) -> Self {
        self.published_ports = true;
        self
    }

    /// Return the client/federation URL for a container that was started with published ports.
    pub async fn url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::PORT).await
    }

    /// Build a small derived image containing the config and start a container from it.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let image_name = "sunbeam-test/tuwunel";
        let descriptor = format!("{image_name}:{}", self.built_tag);

        let dockerfile = format!(
            "FROM {}:{}\nCOPY tuwunel.toml /etc/tuwunel.toml\nENV TUWUNEL_CONFIG=/etc/tuwunel.toml\n",
            Self::NAME,
            self.tag
        );

        util::build_image(
            &descriptor,
            &dockerfile,
            &[("tuwunel.toml", self.config.as_bytes())],
        )
        .await
        .map_err(|e| {
            testcontainers::TestcontainersError::other(format!("build tuwunel image: {e}"))
        })?;

        let mut image = GenericImage::new(image_name, &self.built_tag)
            .with_exposed_port(ContainerPort::Tcp(Self::PORT))
            .with_startup_timeout(Duration::from_secs(120));

        if self.published_ports {
            image = image.with_mapped_port(0, ContainerPort::Tcp(Self::PORT));
        }

        image.start().await
    }
}

impl Default for Tuwunel {
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

    use super::Tuwunel;

    #[tokio::test]
    async fn tuwunel_is_healthy() {
        let container = Tuwunel::default()
            .publish_ports()
            .start()
            .await
            .expect("tuwunel should start");

        let base_url = Tuwunel::url(&container)
            .await
            .expect("tuwunel url should resolve");

        let url = format!("{base_url}/_matrix/client/versions");
        let mut last_status = None;
        for _ in 0..30 {
            match reqwest::get(&url).await {
                Ok(resp) if resp.status().is_success() => {
                    let body: serde_json::Value =
                        resp.json().await.expect("versions body should be json");
                    assert!(
                        body.get("versions").is_some(),
                        "tuwunel /_matrix/client/versions should contain versions"
                    );
                    return;
                }
                other => last_status = other.map(|r| r.status().to_string()).ok(),
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        panic!("tuwunel did not become healthy in time, last status: {last_status:?}");
    }
}
