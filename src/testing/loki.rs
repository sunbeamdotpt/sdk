use std::time::Duration;

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt, core::ContainerPort, runners::AsyncRunner,
};

use crate::testing::util;

/// Testcontainers builder for Loki.
///
/// Defaults to the `grafana/loki:latest` image with the stock single-binary
/// configuration (`-config.file=/etc/loki/local-config.yaml`), usable for
/// push and query integration tests out of the box.
#[derive(Debug, Clone)]
pub struct Loki {
    tag: String,
    published_ports: bool,
}

impl Loki {
    /// Container image name.
    pub const NAME: &'static str = "grafana/loki";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "latest";

    /// HTTP API port.
    pub const PORT: u16 = 3100;

    /// Create a new Loki builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Publish Loki's HTTP port to a random host port so the container is
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

    /// Start a Loki container with the stock single-binary configuration.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let mut image = GenericImage::new(Self::NAME, &self.tag)
            .with_exposed_port(ContainerPort::Tcp(Self::PORT))
            .with_startup_timeout(Duration::from_secs(120));

        if self.published_ports {
            image = image.with_mapped_port(0, ContainerPort::Tcp(Self::PORT));
        }

        image.start().await
    }
}

impl Default for Loki {
    fn default() -> Self {
        Self {
            tag: Self::DEFAULT_TAG.to_owned(),
            published_ports: false,
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use std::time::Duration;

    use super::Loki;

    #[tokio::test]
    async fn loki_is_healthy() {
        let container = Loki::default()
            .publish_ports()
            .start()
            .await
            .expect("loki should start");

        let base_url = Loki::url(&container)
            .await
            .expect("loki url should resolve");

        let url = format!("{base_url}/ready");
        let mut last_status = None;
        for _ in 0..30 {
            match reqwest::get(&url).await {
                Ok(resp) if resp.status().is_success() => return,
                other => last_status = other.map(|r| r.status().to_string()).ok(),
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        panic!("loki did not become healthy in time, last status: {last_status:?}");
    }
}
