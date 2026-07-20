use std::time::Duration;

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt, core::ContainerPort, runners::AsyncRunner,
};

use crate::testing::util;

/// Testcontainers builder for OpenSearch.
///
/// Defaults to the `opensearchproject/opensearch:3` image used by `../sbbb`.
/// Security is disabled and a single-node cluster is configured so the
/// container is usable for integration tests without TLS or authentication.
#[derive(Debug, Clone)]
pub struct OpenSearch {
    tag: String,
    admin_password: String,
    published_ports: bool,
}

impl OpenSearch {
    /// Container image name.
    pub const NAME: &'static str = "opensearchproject/opensearch";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "3";

    /// REST API port.
    pub const REST_PORT: u16 = 9200;
    /// Inter-node transport port.
    pub const TRANSPORT_PORT: u16 = 9300;

    /// Create a new OpenSearch builder with the default configuration.
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

    /// Publish OpenSearch's REST port to a random host port so the container is reachable
    /// without bridge-network access.
    pub fn publish_ports(mut self) -> Self {
        self.published_ports = true;
        self
    }

    /// Return the REST API URL for a container that was started with published ports.
    pub async fn url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::REST_PORT).await
    }

    /// Start a single-node OpenSearch container with security disabled.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let mut image = GenericImage::new(Self::NAME, &self.tag)
            .with_exposed_port(ContainerPort::Tcp(Self::REST_PORT))
            .with_exposed_port(ContainerPort::Tcp(Self::TRANSPORT_PORT))
            .with_env_var("discovery.type", "single-node")
            .with_env_var("DISABLE_SECURITY_PLUGIN", "true")
            .with_env_var(
                "OPENSEARCH_INITIAL_ADMIN_PASSWORD",
                self.admin_password.clone(),
            )
            .with_env_var("OPENSEARCH_JAVA_OPTS", "-Xms512m -Xmx512m")
            .with_env_var("bootstrap.memory_lock", "true")
            .with_startup_timeout(Duration::from_secs(180));

        if self.published_ports {
            image = image.with_mapped_port(0, ContainerPort::Tcp(Self::REST_PORT));
        }

        image.start().await
    }
}

impl Default for OpenSearch {
    fn default() -> Self {
        Self {
            tag: Self::DEFAULT_TAG.to_owned(),
            admin_password: "MyS+ongPwd123".to_owned(),
            published_ports: false,
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use std::time::Duration;

    use super::OpenSearch;

    #[tokio::test]
    async fn opensearch_is_healthy() {
        let container = OpenSearch::default()
            .publish_ports()
            .start()
            .await
            .expect("opensearch should start");

        let base_url = OpenSearch::url(&container)
            .await
            .expect("opensearch url should resolve");

        let url = format!("{base_url}/_cluster/health");

        let mut last_status = None;
        for _ in 0..60 {
            match reqwest::get(&url).await {
                Ok(resp) if resp.status().is_success() => {
                    let body: serde_json::Value =
                        resp.json().await.expect("health body should be json");
                    let status = body["status"].as_str().unwrap_or("unknown");
                    if status == "green" || status == "yellow" {
                        return;
                    }
                    last_status = Some(status.to_string());
                }
                other => {
                    last_status = other.map(|r| r.status().to_string()).ok();
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        panic!("opensearch did not become healthy in time, last status: {last_status:?}");
    }
}
