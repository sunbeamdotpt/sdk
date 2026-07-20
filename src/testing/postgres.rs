use std::time::Duration;

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
};

/// Testcontainers builder for PostgreSQL.
///
/// Defaults to the `postgres:17` image and the `ory/ory/ory` credentials used by
/// the Sunbeam sso-gateway deployment.
#[derive(Debug, Clone)]
pub struct Postgres {
    tag: String,
    user: String,
    password: String,
    db: String,
    network: Option<String>,
    container_name: Option<String>,
    published_port: bool,
}

impl Postgres {
    /// Container image name.
    pub const NAME: &'static str = "postgres";
    /// Default image tag.
    pub const DEFAULT_TAG: &'static str = "17";

    /// PostgreSQL server port.
    pub const PORT: u16 = 5432;

    /// Create a new Postgres builder with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Override the default `ory/ory/ory` credentials and database name.
    pub fn with_credentials(
        mut self,
        user: impl Into<String>,
        password: impl Into<String>,
        db: impl Into<String>,
    ) -> Self {
        self.user = user.into();
        self.password = password.into();
        self.db = db.into();
        self
    }

    /// Publish Postgres's port to a random host port so the container is reachable
    /// without bridge-network access.
    pub fn publish_port(mut self) -> Self {
        self.published_port = true;
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

    /// Return the JDBC-style Postgres URL for a container that was started with a published port.
    pub async fn url(
        container: &ContainerAsync<GenericImage>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let host = container.get_host().await?.to_string();
        let port = container
            .get_host_port_ipv4(ContainerPort::Tcp(Self::PORT))
            .await?;
        Ok(format!(
            "postgres://ory:ory@{host}:{port}/ory?sslmode=disable"
        ))
    }

    /// Start a container from the official Postgres image.
    pub async fn start(
        self,
    ) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
        let mut image = GenericImage::new(Self::NAME, &self.tag)
            .with_exposed_port(ContainerPort::Tcp(Self::PORT))
            .with_wait_for(WaitFor::message_on_stdout(
                "database system is ready to accept connections",
            ))
            .with_env_var("POSTGRES_USER", self.user.clone())
            .with_env_var("POSTGRES_PASSWORD", self.password.clone())
            .with_env_var("POSTGRES_DB", self.db.clone())
            .with_startup_timeout(Duration::from_secs(120));

        if let Some(network) = &self.network {
            image = image.with_network(network);
        }

        if let Some(name) = self.container_name {
            image = image.with_container_name(name);
        }

        if self.published_port {
            image = image.with_mapped_port(0, ContainerPort::Tcp(Self::PORT));
        }

        image.start().await
    }
}

impl Default for Postgres {
    fn default() -> Self {
        Self {
            tag: Self::DEFAULT_TAG.to_owned(),
            user: "ory".to_owned(),
            password: "ory".to_owned(),
            db: "ory".to_owned(),
            network: None,
            container_name: None,
            published_port: false,
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use std::time::Duration;

    use super::Postgres;

    #[tokio::test]
    async fn postgres_publishes_port_and_accepts_connections() {
        let container = Postgres::default()
            .publish_port()
            .start()
            .await
            .expect("postgres should start");

        let url = Postgres::url(&container)
            .await
            .expect("postgres url should resolve");

        // The URL host is the Docker host; try to open a TCP connection to prove the
        // published port is reachable.
        let addr = url
            .trim_start_matches("postgres://ory:ory@")
            .trim_start_matches("postgresql://ory:ory@")
            .split('/')
            .next()
            .expect("url should contain host:port");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            match tokio::net::TcpStream::connect(addr).await {
                Ok(_) => break,
                Err(_) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                Err(e) => panic!("postgres port {addr} should be reachable: {e}"),
            }
        }
    }
}
