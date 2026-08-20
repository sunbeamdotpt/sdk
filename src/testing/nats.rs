use std::time::Duration;

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
};

/// Testcontainers builder for NATS.
///
/// Defaults to the `nats:2.10-alpine` image with JetStream enabled, matching the
/// Sunbeam kanban deployment.
#[derive(Debug, Clone)]
pub struct Nats {
    image_name: String,
    tag: String,
    jetstream: bool,
    network: Option<String>,
    container_name: Option<String>,
    published_port: bool,
}

impl Nats {
    /// Container image name.
    pub const NAME: &'static str = "nats";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "2.10-alpine";

    /// NATS client port.
    pub const PORT: u16 = 4222;

    /// Create a new NATS builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image name (defaults to `nats`).
    pub fn with_image(mut self, name: impl Into<String>) -> Self {
        self.image_name = name.into();
        self
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Enable or disable JetStream (defaults to enabled).
    pub fn with_jetstream(mut self, enabled: bool) -> Self {
        self.jetstream = enabled;
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

    /// Publish NATS's port to a random host port so the container is reachable
    /// without bridge-network access.
    pub fn publish_port(mut self) -> Self {
        self.published_port = true;
        self
    }

    /// Return the `nats://` URL for a container that was started with a published port.
    pub async fn url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let host = container.get_host().await?.to_string();
        let port = container
            .get_host_port_ipv4(ContainerPort::Tcp(Self::PORT))
            .await?;
        Ok(format!("nats://{host}:{port}"))
    }

    /// Start a container from the NATS image.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let mut cmd: Vec<String> = Vec::new();
        if self.jetstream {
            cmd.push("-js".to_owned());
        }

        let mut image = GenericImage::new(&self.image_name, &self.tag)
            .with_exposed_port(ContainerPort::Tcp(Self::PORT))
            .with_wait_for(WaitFor::message_on_stderr("Server is ready"))
            .with_cmd(cmd)
            .with_startup_timeout(Duration::from_secs(120));

        if let Some(network) = &self.network {
            image = image.with_network(network);
        }

        if let Some(name) = &self.container_name {
            image = image.with_container_name(name);
        }

        if self.published_port {
            image = image.with_mapped_port(0, ContainerPort::Tcp(Self::PORT));
        }

        image.start().await
    }
}

impl Default for Nats {
    fn default() -> Self {
        Self {
            image_name: Self::NAME.to_owned(),
            tag: Self::DEFAULT_TAG.to_owned(),
            jetstream: true,
            network: None,
            container_name: None,
            published_port: false,
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use super::Nats;

    #[tokio::test]
    async fn nats_publishes_port_and_becomes_ready() {
        let container = Nats::default()
            .publish_port()
            .start()
            .await
            .expect("nats should start");

        let url = Nats::url(&container)
            .await
            .expect("nats url should resolve");

        // The URL host is the Docker host; try to open a TCP connection to prove the
        // published port is reachable.
        let addr = url
            .trim_start_matches("nats://")
            .split('/')
            .next()
            .expect("url should contain host:port");

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            match tokio::net::TcpStream::connect(addr).await {
                Ok(_) => break,
                Err(_) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
                Err(e) => panic!("nats port {addr} should be reachable: {e}"),
            }
        }
    }
}
