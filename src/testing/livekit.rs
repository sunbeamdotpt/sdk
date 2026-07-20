use std::time::{Duration, SystemTime, UNIX_EPOCH};

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt, core::ContainerPort, runners::AsyncRunner,
};

use crate::testing::util;

/// Minimal LiveKit configuration for integration tests: HTTP API on 7880 with
/// a fixed development API key pair, RTC disabled to keep the container
/// lightweight.
const CONFIG: &str = r#"port: 7880
bind_addresses:
  - "0.0.0.0"
rtc:
  use_external_ip: false
keys:
  devkey: devsecret
logging:
  level: info
"#;

/// Returns a unique tag so parallel test runs don't race building the same image tag.
fn unique_tag() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}")
}

/// Testcontainers builder for LiveKit.
///
/// Defaults to the `livekit/livekit-server:latest` image with a baked-in
/// config using the development key pair [`LiveKit::API_KEY`] /
/// [`LiveKit::API_SECRET`], matching the LiveKit `--dev` conventions.
#[derive(Debug, Clone)]
pub struct LiveKit {
    tag: String,
    config: String,
    built_tag: String,
    published_ports: bool,
}

impl LiveKit {
    /// Container image name.
    pub const NAME: &'static str = "livekit/livekit-server";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "latest";

    /// Development API key baked into the default config.
    pub const API_KEY: &'static str = "devkey";
    /// Development API secret baked into the default config.
    pub const API_SECRET: &'static str = "devsecret";

    /// HTTP / Twirp API port.
    pub const PORT: u16 = 7880;

    /// Create a new LiveKit builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Replace the embedded `livekit.yaml` configuration.
    pub fn with_config(mut self, config: impl Into<String>) -> Self {
        self.config = config.into();
        self
    }

    /// Publish LiveKit's HTTP port to a random host port so the container is
    /// reachable without bridge-network access.
    pub fn publish_ports(mut self) -> Self {
        self.published_ports = true;
        self
    }

    /// Return the HTTP API URL for a container that was started with published ports.
    pub async fn url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::PORT).await
    }

    /// Build a small derived image containing the config and start a container from it.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let image_name = "sunbeam-test/livekit";
        let descriptor = format!("{image_name}:{}", self.built_tag);

        let dockerfile = format!(
            "FROM {}:{}\nCOPY livekit.yaml /etc/livekit.yaml\nCMD [\"--config\", \"/etc/livekit.yaml\"]\n",
            Self::NAME,
            self.tag
        );

        util::build_image(
            &descriptor,
            &dockerfile,
            &[("livekit.yaml", self.config.as_bytes())],
        )
        .await
        .map_err(|e| {
            testcontainers::TestcontainersError::other(format!("build livekit image: {e}"))
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

impl Default for LiveKit {
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

    use super::LiveKit;

    #[tokio::test]
    async fn livekit_is_healthy() {
        let container = LiveKit::default()
            .publish_ports()
            .start()
            .await
            .expect("livekit should start");

        let base_url = LiveKit::url(&container)
            .await
            .expect("livekit url should resolve");

        let mut last_status = None;
        for _ in 0..30 {
            match reqwest::get(&base_url).await {
                Ok(resp) if resp.status().is_success() => return,
                other => last_status = other.map(|r| r.status().to_string()).ok(),
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        panic!("livekit did not become healthy in time, last status: {last_status:?}");
    }
}
