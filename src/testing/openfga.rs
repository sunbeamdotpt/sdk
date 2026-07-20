use std::time::Duration;

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
};

use crate::testing::util;

/// Testcontainers builder for OpenFGA.
///
/// Defaults to the `openfga/openfga:v1.16.0` image used by `../sbbb`.
/// The container starts with the in-memory datastore and authentication disabled,
/// so it is usable for integration tests without any external dependencies.
#[derive(Debug, Clone)]
pub struct OpenFga {
    tag: String,
    datastore_engine: String,
    log_format: String,
    network: Option<String>,
    container_name: Option<String>,
    published_ports: bool,
}

impl OpenFga {
    /// Container image name.
    pub const NAME: &'static str = "openfga/openfga";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "v1.16.0";

    /// HTTP API port.
    pub const HTTP_PORT: u16 = 8080;
    /// gRPC API port.
    pub const GRPC_PORT: u16 = 8081;
    /// Prometheus metrics port.
    pub const METRICS_PORT: u16 = 2112;

    /// Create a new OpenFGA builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Override the datastore engine (defaults to `memory`).
    pub fn with_datastore_engine(mut self, engine: impl Into<String>) -> Self {
        self.datastore_engine = engine.into();
        self
    }

    /// Publish OpenFGA's HTTP port to a random host port so the container is reachable
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

    /// Return the HTTP API URL for a container that was started with published ports.
    pub async fn url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::HTTP_PORT).await
    }

    /// Start an OpenFGA container with the in-memory datastore.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let mut image = GenericImage::new(Self::NAME, &self.tag)
            .with_exposed_port(ContainerPort::Tcp(Self::HTTP_PORT))
            .with_exposed_port(ContainerPort::Tcp(Self::GRPC_PORT))
            .with_exposed_port(ContainerPort::Tcp(Self::METRICS_PORT))
            .with_wait_for(WaitFor::message_on_either_std(
                "starting HTTP server on '0.0.0.0:8080'",
            ))
            .with_env_var("OPENFGA_DATASTORE_ENGINE", self.datastore_engine.clone())
            .with_env_var("OPENFGA_LOG_FORMAT", self.log_format.clone())
            .with_cmd(vec!["run".to_string()])
            .with_startup_timeout(Duration::from_secs(120));

        if let Some(network) = &self.network {
            image = image.with_network(network);
        }

        if let Some(name) = self.container_name {
            image = image.with_container_name(name);
        }

        if self.published_ports {
            image = image.with_mapped_port(0, ContainerPort::Tcp(Self::HTTP_PORT));
        }

        image.start().await
    }
}

impl Default for OpenFga {
    fn default() -> Self {
        Self {
            tag: Self::DEFAULT_TAG.to_owned(),
            datastore_engine: "memory".to_owned(),
            log_format: "text".to_owned(),
            network: None,
            container_name: None,
            published_ports: false,
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use super::OpenFga;

    #[tokio::test]
    async fn openfga_is_healthy() {
        let container = OpenFga::default()
            .publish_ports()
            .start()
            .await
            .expect("openfga should start");

        let url = format!(
            "{}/healthz",
            OpenFga::url(&container)
                .await
                .expect("openfga url should resolve")
        );
        let response = reqwest::get(&url)
            .await
            .expect("health request should succeed");
        assert!(
            response.status().is_success(),
            "openfga /healthz should return 2xx, got {}",
            response.status()
        );
    }
}
