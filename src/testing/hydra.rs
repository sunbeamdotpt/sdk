use std::time::Duration;

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
};

use crate::testing::util;

/// Testcontainers builder for Ory Hydra.
///
/// Defaults to the `oryd/hydra:v25.4.0` image used by `../sbbb`.
#[derive(Debug, Clone)]
pub struct Hydra {
    tag: String,
    dsn: String,
    secrets_system: String,
    urls_self_issuer: String,
    network: Option<String>,
    container_name: Option<String>,
    published_ports: bool,
}

impl Hydra {
    /// Container image name.
    pub const NAME: &'static str = "oryd/hydra";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "v25.4.0";

    /// Public OAuth2 / OIDC port.
    pub const PUBLIC_PORT: u16 = 4444;
    /// Admin API port.
    pub const ADMIN_PORT: u16 = 4445;

    /// Create a new Hydra builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Override the `DSN` value (defaults to `memory`).
    pub fn with_dsn(mut self, dsn: impl Into<String>) -> Self {
        self.dsn = dsn.into();
        self
    }

    /// Override the system secrets value used to encrypt Hydra's databases.
    pub fn with_secrets_system(mut self, secret: impl Into<String>) -> Self {
        self.secrets_system = secret.into();
        self
    }

    /// Override `URLS_SELF_ISSUER` (defaults to `http://localhost:4444`).
    pub fn with_urls_self_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.urls_self_issuer = issuer.into();
        self
    }

    /// Publish Hydra's ports to random host ports so the container is reachable
    /// without bridge-network access.
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

    /// Set the Docker container name so other containers can resolve it by name on the
    /// same network.
    pub fn with_container_name(mut self, name: impl Into<String>) -> Self {
        self.container_name = Some(name.into());
        self
    }

    /// Return the admin API URL for a container that was started with published ports.
    pub async fn admin_url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::ADMIN_PORT).await
    }

    /// Return the public OAuth2/OIDC URL for a container that was started with published ports.
    pub async fn public_url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::PUBLIC_PORT).await
    }

    /// Start a container from the official Hydra image.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let mut image = GenericImage::new(Self::NAME, &self.tag)
            .with_exposed_port(ContainerPort::Tcp(Self::PUBLIC_PORT))
            .with_exposed_port(ContainerPort::Tcp(Self::ADMIN_PORT))
            .with_wait_for(WaitFor::message_on_either_std("Successfully applied"))
            .with_cmd(["serve", "all", "--dev"])
            .with_env_var("DSN", self.dsn.clone())
            .with_env_var("SECRETS_SYSTEM", self.secrets_system.clone())
            .with_env_var("URLS_SELF_ISSUER", self.urls_self_issuer.clone())
            .with_startup_timeout(Duration::from_secs(120));

        if let Some(network) = &self.network {
            image = image.with_network(network);
        }

        if let Some(name) = self.container_name {
            image = image.with_container_name(name);
        }

        if self.published_ports {
            image = image
                .with_mapped_port(0, ContainerPort::Tcp(Self::PUBLIC_PORT))
                .with_mapped_port(0, ContainerPort::Tcp(Self::ADMIN_PORT));
        }

        image.start().await
    }
}

impl Default for Hydra {
    fn default() -> Self {
        Self {
            tag: Self::DEFAULT_TAG.to_owned(),
            dsn: "memory".to_owned(),
            secrets_system: "some-long-secret-key-for-tests".to_owned(),
            urls_self_issuer: "http://localhost:4444".to_owned(),
            network: None,
            container_name: None,
            published_ports: false,
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use super::Hydra;

    #[tokio::test]
    async fn hydra_is_healthy() {
        let container = Hydra::default()
            .publish_ports()
            .start()
            .await
            .expect("hydra should start");

        let url = format!(
            "{}/health/ready",
            Hydra::admin_url(&container)
                .await
                .expect("hydra url should resolve")
        );
        let response = reqwest::get(&url)
            .await
            .expect("health request should succeed");
        assert!(
            response.status().is_success(),
            "hydra /health/ready should return 2xx, got {}",
            response.status()
        );
    }
}
