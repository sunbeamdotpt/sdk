use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt, core::ContainerPort, runners::AsyncRunner,
};
use tokio::time::{Instant, sleep};

use crate::testing::{Hydra, Keto, Kratos, OpenFga, Postgres};

/// Kratos configuration tailored for sso-gateway tests.
///
/// The gateway owns the identity trait model (changelog 2026.07.8), so Kratos persists
/// only the minimal base identity — `{email}` plus credentials — under the base schema
/// id `default` (matching the gateway's `KRATOS_DEFAULT_SCHEMA_ID`). This mirrors
/// `../sso-gateway/deploy/kratos-identity.schema.json`. The registration flow creates a
/// session automatically.
const KRATOS_CONFIG: &str = r#"version: v0.13.0
identity:
  default_schema_id: default
  schemas:
    - id: default
      url: base64://ewogICIkc2NoZW1hIjogImh0dHA6Ly9qc29uLXNjaGVtYS5vcmcvZHJhZnQtMDcvc2NoZW1hIyIsCiAgIiRpZCI6ICJodHRwczovL3NjaGVtYXMuc3VuYmVhbS5wdC9iYXNlLWlkZW50aXR5Lmpzb24iLAogICJ0aXRsZSI6ICJCYXNlIElkZW50aXR5IiwKICAidHlwZSI6ICJvYmplY3QiLAogICJhZGRpdGlvbmFsUHJvcGVydGllcyI6IGZhbHNlLAogICJwcm9wZXJ0aWVzIjogewogICAgInRyYWl0cyI6IHsKICAgICAgInR5cGUiOiAib2JqZWN0IiwKICAgICAgImFkZGl0aW9uYWxQcm9wZXJ0aWVzIjogZmFsc2UsCiAgICAgICJyZXF1aXJlZCI6IFsiZW1haWwiXSwKICAgICAgInByb3BlcnRpZXMiOiB7CiAgICAgICAgImVtYWlsIjogewogICAgICAgICAgInR5cGUiOiAic3RyaW5nIiwKICAgICAgICAgICJmb3JtYXQiOiAiZW1haWwiLAogICAgICAgICAgInRpdGxlIjogIkVtYWlsIiwKICAgICAgICAgICJtYXhMZW5ndGgiOiAzMjAsCiAgICAgICAgICAib3J5LnNoL2tyYXRvcyI6IHsKICAgICAgICAgICAgImNyZWRlbnRpYWxzIjogewogICAgICAgICAgICAgICJwYXNzd29yZCI6IHsgImlkZW50aWZpZXIiOiB0cnVlIH0sCiAgICAgICAgICAgICAgIndlYmF1dGhuIjogeyAiaWRlbnRpZmllciI6IHRydWUgfSwKICAgICAgICAgICAgICAidG90cCI6IHsgImFjY291bnRfbmFtZSI6IHRydWUgfSwKICAgICAgICAgICAgICAiY29kZSI6IHsgImlkZW50aWZpZXIiOiB0cnVlLCAidmlhIjogImVtYWlsIiB9LAogICAgICAgICAgICAgICJwYXNza2V5IjogeyAiZGlzcGxheV9uYW1lIjogdHJ1ZSB9CiAgICAgICAgICAgIH0sCiAgICAgICAgICAgICJyZWNvdmVyeSI6IHsgInZpYSI6ICJlbWFpbCIgfSwKICAgICAgICAgICAgInZlcmlmaWNhdGlvbiI6IHsgInZpYSI6ICJlbWFpbCIgfQogICAgICAgICAgfQogICAgICAgIH0KICAgICAgfQogICAgfQogIH0KfQo=
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
      config:
        haveibeenpwned_enabled: false
  flows:
    registration:
      after:
        password:
          hooks:
            - hook: session
"#;

/// Keto configuration tailored for sso-gateway tests.
const KETO_CONFIG: &str = r#"dsn: memory

namespaces:
  - id: 0
    name: app
  - id: 1
    name: scim_group

serve:
  read:
    host: 0.0.0.0
    port: 4466
  write:
    host: 0.0.0.0
    port: 4467
  metrics:
    host: 0.0.0.0
    port: 4468
"#;

/// Valid ULID used for the system tenant inside the gateway container.
const SYSTEM_TENANT_ULID: &str = "01HZY9JTKKHK3Y6XJJYHZ9Q5TV";

fn unique_prefix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("sso{nanos:x}")
}

/// Permission backend the sso-gateway should use.
///
/// Selects which container the orchestrator starts and which `PERMISSIONS_BACKEND`
/// value the gateway is configured with. The default is [`OpenFga`](Self::OpenFga)
/// because the published gateway image is built with default Cargo features, which
/// include `openfga` but not `keto`.
///
/// Selecting [`Keto`](Self::Keto) requires a gateway image built with the `keto`
/// Cargo feature; otherwise the gateway refuses to start with
/// `PERMISSIONS_BACKEND=keto requires the keto feature`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionBackend {
    /// Ory Keto (relation-tuple) backend.
    Keto,
    /// OpenFGA (ReBAC) backend. Default; matches the published gateway image.
    #[default]
    OpenFga,
}

/// Testcontainers orchestrator for the full sso-gateway stack.
///
/// Starts Postgres, Hydra, Kratos, and the selected permission backend (OpenFGA by
/// default, or Keto) on a private Docker network, then runs a pre-built sso-gateway
/// container against them. The only thing exposed to callers is the gateway's public
/// endpoint URL.
///
/// Use [`with_permissions_backend`](Self::with_permissions_backend) to switch between
/// the OpenFGA and Keto backends.
#[derive(Debug, Clone)]
pub struct SsoGateway {
    image_name: String,
    image_tag: String,
    postgres_tag: String,
    hydra_tag: String,
    kratos_tag: String,
    keto_tag: String,
    openfga_tag: String,
    permissions_backend: PermissionBackend,
    extra_env: HashMap<String, String>,
    network: Option<String>,
}

impl SsoGateway {
    /// Default gateway image name.
    pub const DEFAULT_IMAGE_NAME: &'static str = "ghcr.io/sunbeamdotpt/sso-gateway";
    /// Default gateway image tag.
    pub const DEFAULT_IMAGE_TAG: &'static str = "latest";

    /// Gateway HTTP port inside the container.
    pub const PORT: u16 = 8080;

    /// Create a new orchestrator with the default pre-built gateway image.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the gateway image name and tag.
    pub fn with_image(mut self, name: impl Into<String>, tag: impl Into<String>) -> Self {
        self.image_name = name.into();
        self.image_tag = tag.into();
        self
    }

    /// Override the Postgres image tag.
    pub fn with_postgres_tag(mut self, tag: impl Into<String>) -> Self {
        self.postgres_tag = tag.into();
        self
    }

    /// Override the Hydra image tag.
    pub fn with_hydra_tag(mut self, tag: impl Into<String>) -> Self {
        self.hydra_tag = tag.into();
        self
    }

    /// Override the Kratos image tag.
    pub fn with_kratos_tag(mut self, tag: impl Into<String>) -> Self {
        self.kratos_tag = tag.into();
        self
    }

    /// Override the Keto image tag. Only used when the Keto backend is selected.
    pub fn with_keto_tag(mut self, tag: impl Into<String>) -> Self {
        self.keto_tag = tag.into();
        self
    }

    /// Override the OpenFGA image tag. Only used when the OpenFGA backend is selected.
    pub fn with_openfga_tag(mut self, tag: impl Into<String>) -> Self {
        self.openfga_tag = tag.into();
        self
    }

    /// Select the permission backend the gateway uses (OpenFGA by default).
    ///
    /// Selecting [`PermissionBackend::Keto`] requires a gateway image built with the
    /// `keto` Cargo feature.
    pub fn with_permissions_backend(mut self, backend: PermissionBackend) -> Self {
        self.permissions_backend = backend;
        self
    }

    /// Inject an extra environment variable into the sso-gateway container.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_env.insert(key.into(), value.into());
        self
    }

    /// Attach the stack's containers to a specific Docker network instead of a
    /// per-stack one.
    ///
    /// Use this when other containers (e.g. a service under test) must reach
    /// the gateway by container name; [`SsoGatewayHandle::internal_url`] then
    /// gives the gateway's in-network address. The caller is responsible for
    /// the network's lifecycle — it is created implicitly by the first
    /// container started on it.
    pub fn with_network(mut self, network: impl Into<String>) -> Self {
        self.network = Some(network.into());
        self
    }

    /// Start the full stack and return a handle that exposes only the gateway endpoint.
    pub async fn start(self) -> Result<SsoGatewayHandle, testcontainers::TestcontainersError> {
        let prefix = unique_prefix();
        let network = self
            .network
            .clone()
            .unwrap_or_else(|| format!("{prefix}-net"));

        let postgres_name = format!("{prefix}-postgres");
        let hydra_name = format!("{prefix}-hydra");
        let kratos_name = format!("{prefix}-kratos");
        let keto_name = format!("{prefix}-keto");
        let openfga_name = format!("{prefix}-openfga");
        let gateway_name = format!("{prefix}-gateway");

        // Secrets are derived from the prefix so parallel stacks do not share them and
        // so they are never the literal development values denied by the gateway.
        let state_cookie_secret = format!("sunbeam-test-{prefix}-secret-key-at-least-32-bytes");
        let bootstrap_client_secret = format!("sunbeam-test-{prefix}-bootstrap-secret-key");

        // Start the backing services sequentially so the shared network is created before
        // the gateway container joins it.
        let postgres = Postgres::new()
            .with_tag(&self.postgres_tag)
            .with_network(&network)
            .with_container_name(&postgres_name)
            .start()
            .await?;

        let _kratos = Kratos::new()
            .with_tag(&self.kratos_tag)
            .with_network(&network)
            .with_container_name(&kratos_name)
            .with_config(KRATOS_CONFIG)
            .start()
            .await?;

        let hydra_issuer = format!("http://{hydra_name}:4444");
        let _hydra = Hydra::new()
            .with_tag(&self.hydra_tag)
            .with_network(&network)
            .with_container_name(&hydra_name)
            .with_urls_self_issuer(&hydra_issuer)
            .start()
            .await?;

        // Start the selected permission backend and collect the env vars that point the
        // gateway at it. Only the chosen backend's container is run.
        let (permission_backend, backend_env) = match self.permissions_backend {
            PermissionBackend::Keto => {
                let keto = Keto::new()
                    .with_tag(&self.keto_tag)
                    .with_network(&network)
                    .with_container_name(&keto_name)
                    .with_config(KETO_CONFIG)
                    .start()
                    .await?;
                let env = vec![
                    ("PERMISSIONS_BACKEND".to_string(), "keto".to_string()),
                    (
                        "KETO_READ_URL".to_string(),
                        format!("http://{keto_name}:{}", Keto::READ_PORT),
                    ),
                    (
                        "KETO_WRITE_URL".to_string(),
                        format!("http://{keto_name}:{}", Keto::WRITE_PORT),
                    ),
                    (
                        "KETO_GRPC_URL".to_string(),
                        format!("http://{keto_name}:4469"),
                    ),
                ];
                (keto, env)
            }
            PermissionBackend::OpenFga => {
                let openfga = OpenFga::new()
                    .with_tag(&self.openfga_tag)
                    .with_network(&network)
                    .with_container_name(&openfga_name)
                    .start()
                    .await?;
                let env = vec![
                    ("PERMISSIONS_BACKEND".to_string(), "openfga".to_string()),
                    (
                        "OPENFGA_URL".to_string(),
                        format!("http://{openfga_name}:{}", OpenFga::HTTP_PORT),
                    ),
                ];
                (openfga, env)
            }
        };

        // Give the backing services a moment to finish opening their HTTP ports before
        // the gateway starts connecting to them.
        sleep(Duration::from_secs(2)).await;

        let database_url = format!("postgres://ory:ory@{postgres_name}:5432/ory?sslmode=disable");

        let mut image = GenericImage::new(&self.image_name, &self.image_tag)
            .with_exposed_port(ContainerPort::Tcp(Self::PORT))
            .with_mapped_port(0, ContainerPort::Tcp(Self::PORT))
            .with_container_name(&gateway_name)
            .with_network(&network)
            .with_env_var("BIND_ADDR", "0.0.0.0:8080")
            .with_env_var("DATABASE_URL", database_url)
            .with_env_var("DATABASE_SSL_REQUIRED", "false")
            .with_env_var("HYDRA_ADMIN_URL", format!("http://{hydra_name}:4445"))
            .with_env_var("HYDRA_PUBLIC_URL", format!("http://{hydra_name}:4444"))
            .with_env_var("KRATOS_ADMIN_URL", format!("http://{kratos_name}:4434"))
            .with_env_var("KRATOS_PUBLIC_URL", format!("http://{kratos_name}:4433"))
            // Matches the base schema id baked into KRATOS_CONFIG; the gateway owns the
            // trait model and only persists `{email}` to Kratos.
            .with_env_var("KRATOS_DEFAULT_SCHEMA_ID", "default")
            .with_env_var("SYSTEM_TENANT_ULID", SYSTEM_TENANT_ULID)
            .with_env_var("SYSTEM_BOOTSTRAP_CLIENT_ID", "system-bootstrap-client")
            .with_env_var("SYSTEM_BOOTSTRAP_CLIENT_SECRET", &bootstrap_client_secret)
            .with_env_var("STATE_COOKIE_SECRET", &state_cookie_secret)
            // PUBLIC_BASE_URL is intentionally a placeholder because the host port is not
            // known until after the container starts. This is fine for Connect-RPC tests;
            // browser OAuth/SAML flows that rely on redirects may need additional setup.
            .with_env_var("PUBLIC_BASE_URL", "http://127.0.0.1:8080")
            .with_env_var("ALLOWED_RETURN_TO_HOSTS", "127.0.0.1,localhost")
            .with_env_var("COOKIE_SECURE", "true")
            .with_env_var("COOKIE_SAMESITE", "Lax")
            .with_startup_timeout(Duration::from_secs(180));

        for (key, value) in &backend_env {
            image = image.with_env_var(key, value);
        }
        for (key, value) in &self.extra_env {
            image = image.with_env_var(key, value);
        }

        let gateway = image.start().await?;

        let host = gateway
            .get_host()
            .await
            .map_err(|e| testcontainers::TestcontainersError::other(format!("host: {e}")))?
            .to_string();
        let host_port = gateway
            .get_host_port_ipv4(ContainerPort::Tcp(Self::PORT))
            .await
            .map_err(|e| testcontainers::TestcontainersError::other(format!("port: {e}")))?;
        let endpoint = format!("http://{host}:{host_port}");

        wait_for_gateway(&endpoint)
            .await
            .map_err(|e| testcontainers::TestcontainersError::other(format!("ready: {e}")))?;

        Ok(SsoGatewayHandle {
            endpoint,
            internal_url: format!("http://{gateway_name}:{}", Self::PORT),
            _postgres: postgres,
            _hydra,
            _kratos,
            _permission_backend: permission_backend,
            gateway,
        })
    }
}

impl Default for SsoGateway {
    fn default() -> Self {
        Self {
            image_name: Self::DEFAULT_IMAGE_NAME.to_owned(),
            image_tag: Self::DEFAULT_IMAGE_TAG.to_owned(),
            postgres_tag: Postgres::DEFAULT_TAG.to_owned(),
            hydra_tag: Hydra::DEFAULT_TAG.to_owned(),
            kratos_tag: Kratos::DEFAULT_TAG.to_owned(),
            keto_tag: Keto::DEFAULT_TAG.to_owned(),
            openfga_tag: OpenFga::DEFAULT_TAG.to_owned(),
            permissions_backend: PermissionBackend::default(),
            extra_env: HashMap::new(),
            network: None,
        }
    }
}

/// A running sso-gateway stack.
///
/// Dropping this value stops and removes all containers. The only public information is
/// the gateway endpoint address.
pub struct SsoGatewayHandle {
    endpoint: String,
    internal_url: String,
    #[allow(dead_code)]
    _postgres: ContainerAsync<GenericImage>,
    #[allow(dead_code)]
    _hydra: ContainerAsync<GenericImage>,
    #[allow(dead_code)]
    _kratos: ContainerAsync<GenericImage>,
    #[allow(dead_code)]
    _permission_backend: ContainerAsync<GenericImage>,
    gateway: ContainerAsync<GenericImage>,
}

impl SsoGatewayHandle {
    /// Return the gateway's public HTTP endpoint.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Return the gateway's URL for other containers on the same Docker
    /// network (`http://<gateway-container-name>:8080`).
    ///
    /// Use this for service containers that must call the gateway from inside
    /// Docker (started with [`SsoGateway::with_network`]); from the test
    /// process itself, use [`endpoint`](Self::endpoint).
    pub fn internal_url(&self) -> &str {
        &self.internal_url
    }

    /// Stop the gateway container and drop the stack.
    ///
    /// The containers are removed when the handle is dropped; this method is provided
    /// as an explicit hook for tests that want to await cleanup.
    pub async fn shutdown(self) {
        drop(self.gateway);
        // Give testcontainers a moment to schedule removals before returning.
        sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_gateway(endpoint: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let url = format!("{endpoint}/.well-known/openid-configuration");
    let deadline = Instant::now() + Duration::from_secs(60);

    loop {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            _ => {
                if Instant::now() >= deadline {
                    return Err(format!("gateway did not become ready at {url}").into());
                }
                sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use super::SsoGateway;

    #[tokio::test]
    #[ignore = "requires a pre-built sso-gateway image (see SsoGateway::DEFAULT_IMAGE_NAME)"]
    async fn sso_gateway_stack_exposes_ready_endpoint() {
        let tag =
            std::env::var("SSO_GATEWAY_IMAGE_TAG").unwrap_or_else(|_| "v2026.07.20".to_string());
        let gateway = SsoGateway::new()
            .with_image(SsoGateway::DEFAULT_IMAGE_NAME, &tag)
            .start()
            .await
            .expect("sso-gateway stack should start");

        let endpoint = gateway.endpoint();

        let resp = reqwest::get(format!("{endpoint}/.well-known/openid-configuration"))
            .await
            .expect("gateway should accept request");

        assert!(
            resp.status().is_success(),
            "gateway OIDC discovery should be successful: {}",
            resp.status()
        );
    }
}
