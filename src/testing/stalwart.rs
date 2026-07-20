use std::time::Duration;

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt, core::ContainerPort, runners::AsyncRunner,
};

use crate::testing::util;

/// Testcontainers builder for Stalwart Mail Server.
///
/// Defaults to the `stalwartlabs/stalwart:v0.15.5` image used by `../sbbb`.
/// The container starts in bootstrap mode and exposes the web admin / JMAP
/// listener on port 8080.
#[derive(Debug, Clone)]
pub struct Stalwart {
    tag: String,
    published_ports: bool,
}

impl Stalwart {
    /// Container image name.
    pub const NAME: &'static str = "stalwartlabs/stalwart";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "v0.15.5";

    /// Web admin + JMAP port.
    pub const PORT: u16 = 8080;

    /// Create a new Stalwart builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Publish Stalwart's port to a random host port so the container is reachable
    /// without bridge-network access.
    pub fn publish_ports(mut self) -> Self {
        self.published_ports = true;
        self
    }

    /// Return the web admin / JMAP URL for a container that was started with published ports.
    pub async fn url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        util::container_host_url(container, Self::PORT).await
    }

    /// Start a Stalwart container in bootstrap mode.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let mut image = GenericImage::new(Self::NAME, &self.tag)
            .with_exposed_port(ContainerPort::Tcp(Self::PORT))
            .with_startup_timeout(Duration::from_secs(90));

        if self.published_ports {
            image = image.with_mapped_port(0, ContainerPort::Tcp(Self::PORT));
        }

        image.start().await
    }

    /// Extract the temporary bootstrap admin password from the container logs.
    ///
    /// The v0.15.5 image prints a random password on first start. This helper
    /// parses it so tests can authenticate against the admin API if needed.
    pub async fn admin_password(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let stdout = container.stdout_to_vec().await?;
        let stderr = container.stderr_to_vec().await?;
        let logs = String::from_utf8_lossy(&stdout).to_string() + &String::from_utf8_lossy(&stderr);

        let marker = "Your administrator account is 'admin' with password '";
        let start = logs
            .find(marker)
            .ok_or("admin password banner not found in stalwart logs")?
            + marker.len();
        let rest = &logs[start..];
        let end = rest.find('\'').ok_or("malformed admin password banner")?;
        Ok(rest[..end].to_string())
    }
}

impl Default for Stalwart {
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

    use super::Stalwart;

    #[tokio::test]
    async fn stalwart_is_healthy() {
        let container = Stalwart::default()
            .publish_ports()
            .start()
            .await
            .expect("stalwart should start");

        let base_url = Stalwart::url(&container)
            .await
            .expect("stalwart url should resolve");

        let url = format!("{base_url}/login");
        let mut last_status = None;
        for _ in 0..30 {
            match reqwest::get(&url).await {
                Ok(resp) if resp.status().is_success() => {
                    last_status = Some(resp.status().to_string());
                    break;
                }
                other => last_status = other.map(|r| r.status().to_string()).ok(),
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        let status = last_status.expect("stalwart /login should respond");
        assert!(
            status.starts_with('2'),
            "stalwart /login should return 2xx, got {status}"
        );

        // Also verify we can extract the bootstrap admin password from the logs.
        let password = Stalwart::admin_password(&container)
            .await
            .expect("should parse admin password");
        assert!(!password.is_empty(), "admin password should not be empty");
    }
}
