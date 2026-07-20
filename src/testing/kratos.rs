use std::time::{Duration, SystemTime, UNIX_EPOCH};

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
};

use crate::testing::util;

/// Minimal Kratos configuration that lets the container start with an in-memory DSN.
const CONFIG: &str = r#"version: v0.13.0
identity:
  default_schema_id: default
  schemas:
    - id: default
      url: base64://eyIkaWQiOiAiaHR0cHM6Ly9zY2hlbWFzLm9yeS5zaC9wcmVzZXRzL2tyYXRvcy9xdWlja3N0YXJ0L2VtYWlsLXBhc3N3b3JkL2lkZW50aXR5LnNjaGVtYS5qc29uIiwgIiRzY2hlbWEiOiAiaHR0cDovL2pzb24tc2NoZW1hLm9yZy9kcmFmdC0wNy9zY2hlbWEjIiwgInRpdGxlIjogIlBlcnNvbiIsICJ0eXBlIjogIm9iamVjdCIsICJwcm9wZXJ0aWVzIjogeyJ0cmFpdHMiOiB7InR5cGUiOiAib2JqZWN0IiwgInByb3BlcnRpZXMiOiB7ImVtYWlsIjogeyJ0eXBlIjogInN0cmluZyIsICJmb3JtYXQiOiAiZW1haWwiLCAidGl0bGUiOiAiRS1NYWlsIiwgIm9yeS5zaC9rcmF0b3MiOiB7ImNyZWRlbnRpYWxzIjogeyJwYXNzd29yZCI6IHsiaWRlbnRpZmllciI6IHRydWV9fSwgInJlY292ZXJ5IjogeyJ2aWEiOiAiZW1haWwifSwgInZlcmlmaWNhdGlvbiI6IHsidmlhIjogImVtYWlsIn19fX0sICJyZXF1aXJlZCI6IFsiZW1haWwiXSwgImFkZGl0aW9uYWxQcm9wZXJ0aWVzIjogZmFsc2V9fX0=
serve:
  public:
    base_url: http://localhost:4433/
  admin:
    base_url: http://localhost:4434/
selfservice:
  default_browser_return_url: http://localhost:4433/
  methods:
    password:
      enabled: true
  flows:
    error:
      ui_url: http://localhost:4433/error
"#;

/// Returns a unique tag so parallel test runs don't race building the same image tag.
fn unique_tag() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}")
}

/// Testcontainers builder for Ory Kratos.
///
/// Defaults to the `oryd/kratos:v25.4.0` image used by `../sbbb`.
#[derive(Debug, Clone)]
pub struct Kratos {
    tag: String,
    dsn: String,
    config: String,
    built_tag: String,
    network: Option<String>,
    container_name: Option<String>,
    published_ports: bool,
}

impl Kratos {
    /// Container image name.
    pub const NAME: &'static str = "oryd/kratos";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "v25.4.0";

    /// Public (self-service / frontend) API port.
    pub const PUBLIC_PORT: u16 = 4433;
    /// Admin API port.
    pub const ADMIN_PORT: u16 = 4434;

    /// Create a new Kratos builder with the default configuration.
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

    /// Replace the embedded `kratos.yml` configuration.
    pub fn with_config(mut self, config: impl Into<String>) -> Self {
        self.config = config.into();
        self
    }

    /// Publish Kratos's ports to random host ports so the container is reachable
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

    /// Return the public API URL for a container that was started with published ports.
    pub async fn public_url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::PUBLIC_PORT).await
    }

    /// Build a small derived image containing the config and start a container from it.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let image_name = "sunbeam-test/kratos";
        let descriptor = format!("{image_name}:{}", self.built_tag);

        let dockerfile = format!(
            "FROM {}:{}\nCOPY kratos.yml /home/ory/kratos.yml\nCMD [\"serve\", \"-c\", \"/home/ory/kratos.yml\", \"--dev\"]\n",
            Self::NAME,
            self.tag
        );

        util::build_image(
            &descriptor,
            &dockerfile,
            &[("kratos.yml", self.config.as_bytes())],
        )
        .await
        .map_err(|e| {
            testcontainers::TestcontainersError::other(format!("build kratos image: {e}"))
        })?;

        let mut image = GenericImage::new(image_name, &self.built_tag)
            .with_exposed_port(ContainerPort::Tcp(Self::PUBLIC_PORT))
            .with_exposed_port(ContainerPort::Tcp(Self::ADMIN_PORT))
            .with_wait_for(WaitFor::message_on_either_std(
                "Starting the public httpd on: 0.0.0.0:4433",
            ))
            .with_env_var("DSN", self.dsn.clone())
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

impl Default for Kratos {
    fn default() -> Self {
        Self {
            tag: Self::DEFAULT_TAG.to_owned(),
            dsn: "memory".to_owned(),
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
    use super::Kratos;

    #[tokio::test]
    async fn kratos_is_healthy() {
        let container = Kratos::default()
            .publish_ports()
            .start()
            .await
            .expect("kratos should start");

        let url = format!(
            "{}/health/ready",
            Kratos::public_url(&container)
                .await
                .expect("kratos url should resolve")
        );
        let response = reqwest::get(&url)
            .await
            .expect("health request should succeed");
        assert!(
            response.status().is_success(),
            "kratos /health/ready should return 2xx, got {}",
            response.status()
        );
    }
}
