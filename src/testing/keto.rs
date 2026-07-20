use std::time::{Duration, SystemTime, UNIX_EPOCH};

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
};

use crate::testing::util;

/// Default Keto configuration that lets the container start with an in-memory DSN.
const CONFIG: &str = include_str!("keto.yml");

/// Returns a unique tag so parallel test runs don't race building the same image tag.
fn unique_tag() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}")
}

/// Testcontainers builder for Ory Keto.
///
/// Defaults to the `oryd/keto:v26.2.0` image used by `../sbbb`.
#[derive(Debug, Clone)]
pub struct Keto {
    tag: String,
    config: String,
    built_tag: String,
    network: Option<String>,
    container_name: Option<String>,
    published_ports: bool,
}

impl Keto {
    /// Container image name.
    pub const NAME: &'static str = "oryd/keto";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "v26.2.0";

    /// Read / check API port.
    pub const READ_PORT: u16 = 4466;
    /// Write / admin API port.
    pub const WRITE_PORT: u16 = 4467;
    /// Metrics port.
    pub const METRICS_PORT: u16 = 4468;

    /// Create a new Keto builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Replace the embedded `keto.yml` configuration.
    pub fn with_config(mut self, config: impl Into<String>) -> Self {
        self.config = config.into();
        self
    }

    /// Publish Keto's read/write ports to random host ports so the container is reachable
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

    /// Return the read/check API URL for a container that was started with published ports.
    pub async fn read_url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::READ_PORT).await
    }

    /// Return the write/admin API URL for a container that was started with published ports.
    pub async fn write_url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::WRITE_PORT).await
    }

    /// Build a small derived image containing the config and start a container from it.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let image_name = "sunbeam-test/keto";
        let descriptor = format!("{image_name}:{}", self.built_tag);

        let dockerfile = format!(
            "FROM {}:{}\nCOPY keto.yml /home/ory/keto.yml\nCMD [\"serve\", \"-c\", \"/home/ory/keto.yml\"]\n",
            Self::NAME,
            self.tag
        );

        util::build_image(
            &descriptor,
            &dockerfile,
            &[("keto.yml", self.config.as_bytes())],
        )
        .await
        .map_err(|e| {
            testcontainers::TestcontainersError::other(format!("build keto image: {e}"))
        })?;

        let mut image = GenericImage::new(image_name, &self.built_tag)
            .with_exposed_port(ContainerPort::Tcp(Self::READ_PORT))
            .with_exposed_port(ContainerPort::Tcp(Self::WRITE_PORT))
            .with_exposed_port(ContainerPort::Tcp(Self::METRICS_PORT))
            .with_wait_for(WaitFor::message_on_either_std("Successfully applied"))
            .with_env_var("DSN", "memory")
            .with_startup_timeout(Duration::from_secs(120));

        if let Some(network) = &self.network {
            image = image.with_network(network);
        }

        if let Some(name) = self.container_name {
            image = image.with_container_name(name);
        }

        if self.published_ports {
            image = image
                .with_mapped_port(0, ContainerPort::Tcp(Self::READ_PORT))
                .with_mapped_port(0, ContainerPort::Tcp(Self::WRITE_PORT));
        }

        image.start().await
    }
}

impl Default for Keto {
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
    use super::Keto;

    #[tokio::test]
    async fn keto_is_healthy() {
        let container = Keto::default()
            .publish_ports()
            .start()
            .await
            .expect("keto should start");

        let url = format!(
            "{}/health/ready",
            Keto::read_url(&container)
                .await
                .expect("keto url should resolve")
        );
        let response = reqwest::get(&url)
            .await
            .expect("health request should succeed");
        assert!(
            response.status().is_success(),
            "keto /health/ready should return 2xx, got {}",
            response.status()
        );
    }
}
