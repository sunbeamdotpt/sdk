use std::time::Duration;

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
};

use crate::testing::util;

/// Testcontainers builder for OpenBao.
///
/// Defaults to the `openbao/openbao:2.5.1` image used by `../sbbb`.
/// The container starts in dev mode, which auto-initialises and auto-unseals
/// the server, so no manual unseal step is required.
#[derive(Debug, Clone)]
pub struct OpenBao {
    tag: String,
    root_token: String,
    published_ports: bool,
}

impl OpenBao {
    /// Container image name.
    pub const NAME: &'static str = "openbao/openbao";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "2.5.1";

    /// API/UI port.
    pub const PORT: u16 = 8200;
    /// Default root token used in dev mode.
    pub const DEFAULT_ROOT_TOKEN: &'static str = "root";

    /// Create a new OpenBao builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Override the dev-mode root token (defaults to `root`).
    pub fn with_root_token(mut self, token: impl Into<String>) -> Self {
        self.root_token = token.into();
        self
    }

    /// Publish OpenBao's port to a random host port so the container is reachable
    /// without bridge-network access.
    pub fn publish_ports(mut self) -> Self {
        self.published_ports = true;
        self
    }

    /// Return the API/UI URL for a container that was started with a published port.
    pub async fn url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::PORT).await
    }

    /// Start a container from the official OpenBao image in dev mode.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let root_token_arg = format!("-dev-root-token-id={}", self.root_token);
        let mut image = GenericImage::new(Self::NAME, &self.tag)
            .with_exposed_port(ContainerPort::Tcp(Self::PORT))
            .with_wait_for(WaitFor::message_on_either_std(
                "OpenBao server started! Log data will stream in below:",
            ))
            .with_cmd(vec![
                "server".to_string(),
                "-dev".to_string(),
                root_token_arg,
                "-dev-listen-address=0.0.0.0:8200".to_string(),
            ])
            .with_env_var("BAO_DEV_ROOT_TOKEN_ID", self.root_token.clone())
            .with_env_var("BAO_DEV_LISTEN_ADDRESS", "0.0.0.0:8200")
            .with_startup_timeout(Duration::from_secs(120));

        if self.published_ports {
            image = image.with_mapped_port(0, ContainerPort::Tcp(Self::PORT));
        }

        image.start().await
    }
}

impl Default for OpenBao {
    fn default() -> Self {
        Self {
            tag: Self::DEFAULT_TAG.to_owned(),
            root_token: Self::DEFAULT_ROOT_TOKEN.to_owned(),
            published_ports: false,
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use super::OpenBao;

    #[tokio::test]
    async fn openbao_is_healthy() {
        let container = OpenBao::default()
            .publish_ports()
            .start()
            .await
            .expect("openbao should start");

        let base_url = OpenBao::url(&container)
            .await
            .expect("openbao url should resolve");

        let url = format!("{base_url}/v1/sys/health");
        let response = reqwest::get(&url)
            .await
            .expect("health request should succeed");
        assert!(
            response.status().is_success(),
            "openbao /v1/sys/health should return 2xx, got {}",
            response.status()
        );
    }
}
