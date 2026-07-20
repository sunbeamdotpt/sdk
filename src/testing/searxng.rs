use std::time::Duration;

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
};

use crate::testing::util;

/// Testcontainers builder for SearXNG.
///
/// Defaults to the `searxng/searxng:latest` image used by `../sbbb`.
#[derive(Debug, Clone)]
pub struct SearXng {
    tag: String,
    base_url: String,
    published_ports: bool,
}

impl SearXng {
    /// Container image name.
    pub const NAME: &'static str = "searxng/searxng";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "latest";

    /// Web / API port.
    pub const PORT: u16 = 8080;

    /// Create a new SearXNG builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Override the `BASE_URL` value (defaults to `http://localhost:8080/`).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Publish SearXNG's port to a random host port so the container is reachable
    /// without bridge-network access.
    pub fn publish_ports(mut self) -> Self {
        self.published_ports = true;
        self
    }

    /// Return the web/API URL for a container that was started with published ports.
    pub async fn url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::PORT).await
    }

    /// Start a SearXNG container.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let mut image = GenericImage::new(Self::NAME, &self.tag)
            .with_exposed_port(ContainerPort::Tcp(Self::PORT))
            .with_wait_for(WaitFor::message_on_either_std("Started worker-1"))
            .with_env_var("BASE_URL", self.base_url.clone())
            .with_env_var("INSTANCE_NAME", "searxng")
            .with_env_var("GRANIAN_HOST", "0.0.0.0")
            .with_startup_timeout(Duration::from_secs(120));

        if self.published_ports {
            image = image.with_mapped_port(0, ContainerPort::Tcp(Self::PORT));
        }

        image.start().await
    }
}

impl Default for SearXng {
    fn default() -> Self {
        Self {
            tag: Self::DEFAULT_TAG.to_owned(),
            base_url: "http://localhost:8080/".to_owned(),
            published_ports: false,
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use super::SearXng;

    #[tokio::test]
    async fn searxng_is_healthy() {
        let container = SearXng::default()
            .publish_ports()
            .start()
            .await
            .expect("searxng should start");

        let base_url = SearXng::url(&container)
            .await
            .expect("searxng url should resolve");

        let url = format!("{base_url}/healthz");
        let response = reqwest::get(&url)
            .await
            .expect("health request should succeed");
        assert!(
            response.status().is_success(),
            "searxng /healthz should return 2xx, got {}",
            response.status()
        );
    }
}
