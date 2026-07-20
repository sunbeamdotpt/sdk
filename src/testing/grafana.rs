use std::time::Duration;

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt, core::ContainerPort, runners::AsyncRunner,
};

use crate::testing::util;

/// Testcontainers builder for Grafana.
///
/// Defaults to the `grafana/grafana:latest` image with the stock
/// configuration and the default `admin` / `admin` credentials (overridable
/// with `.with_admin_password(...)`).
#[derive(Debug, Clone)]
pub struct Grafana {
    tag: String,
    admin_user: String,
    admin_password: String,
    published_ports: bool,
}

impl Grafana {
    /// Container image name.
    pub const NAME: &'static str = "grafana/grafana";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "latest";

    /// Default admin username.
    pub const ADMIN_USER: &'static str = "admin";
    /// Default admin password.
    pub const ADMIN_PASSWORD: &'static str = "admin";

    /// HTTP API port.
    pub const PORT: u16 = 3000;

    /// Create a new Grafana builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Override the initial admin password.
    pub fn with_admin_password(mut self, password: impl Into<String>) -> Self {
        self.admin_password = password.into();
        self
    }

    /// Publish Grafana's HTTP port to a random host port so the container is
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

    /// Start a Grafana container with the stock configuration.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let mut image = GenericImage::new(Self::NAME, &self.tag)
            .with_exposed_port(ContainerPort::Tcp(Self::PORT))
            .with_env_var("GF_SECURITY_ADMIN_USER", self.admin_user.clone())
            .with_env_var("GF_SECURITY_ADMIN_PASSWORD", self.admin_password.clone())
            .with_env_var("GF_AUTH_ANONYMOUS_ENABLED", "false")
            .with_startup_timeout(Duration::from_secs(120));

        if self.published_ports {
            image = image.with_mapped_port(0, ContainerPort::Tcp(Self::PORT));
        }

        image.start().await
    }
}

impl Default for Grafana {
    fn default() -> Self {
        Self {
            tag: Self::DEFAULT_TAG.to_owned(),
            admin_user: Self::ADMIN_USER.to_owned(),
            admin_password: Self::ADMIN_PASSWORD.to_owned(),
            published_ports: false,
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use std::time::Duration;

    use super::Grafana;

    #[tokio::test]
    async fn grafana_is_healthy() {
        let container = Grafana::default()
            .publish_ports()
            .start()
            .await
            .expect("grafana should start");

        let base_url = Grafana::url(&container)
            .await
            .expect("grafana url should resolve");

        let url = format!("{base_url}/api/health");
        let mut last_status = None;
        for _ in 0..30 {
            match reqwest::get(&url).await {
                Ok(resp) if resp.status().is_success() => return,
                other => last_status = other.map(|r| r.status().to_string()).ok(),
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        panic!("grafana did not become healthy in time, last status: {last_status:?}");
    }
}
