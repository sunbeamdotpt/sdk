//! Kanban server stack container builder.
//!
//! Boots everything the kanban server needs on a shared Docker network —
//! Postgres, NATS (JetStream), OpenSearch, MinIO, and the full sso-gateway
//! stack — provisions a `kanban-test` tenant and a service application with
//! `permission:admin` + `tenant:admin` via the IAM API, creates the MinIO
//! attachments bucket, and then starts the kanban server image.
//!
//! Requires the `auth` feature (IAM provisioning uses the generated
//! sso-gateway client). Usage mirrors [`SsoGateway`](crate::testing::SsoGateway):
//!
//! ```rust,no_run
//! # #[tokio::main]
//! # async fn main() {
//! let stack = sdk::testing::Kanban::new().start().await.unwrap();
//! let endpoint = stack.endpoint(); // http://127.0.0.1:<random-port>
//! // Mint tokens via stack.sso_gateway_endpoint() and call the ConnectRPC
//! // API with sdk::kanban::KanbanClient.
//! # }
//! ```

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt, core::ContainerPort, runners::AsyncRunner,
};
use tokio::time::{Instant, sleep};

use crate::auth::{AuthClient, v1};
use crate::testing::{Nats, OpenSearch, Postgres, SsoGateway, SsoGatewayHandle, util};

/// Error type used by the provisioning and bucket-creation helpers.
type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Bootstrap client id configured by the sso-gateway orchestrator.
const SYSTEM_BOOTSTRAP_CLIENT_ID: &str = "system-bootstrap-client";

/// MinIO bucket the kanban server stores attachments in (its `S3_BUCKET`
/// default). The server does not create the bucket itself.
const S3_BUCKET: &str = "sunbeam-kanban";

/// MinIO root credentials used inside the throwaway stack.
const MINIO_ROOT_USER: &str = "minioadmin";
const MINIO_ROOT_PASSWORD: &str = "minioadmin";

fn unique_prefix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("kb{nanos:x}")
}

/// Testcontainers orchestrator for the full kanban server stack.
///
/// Starts Postgres, NATS, OpenSearch, MinIO, and an sso-gateway stack on a
/// private Docker network, provisions the service credentials the server
/// needs, and runs a pre-built kanban image against them. The only thing
/// exposed to callers is the server's public endpoint plus the credentials
/// tests need to mint their own user tokens.
#[derive(Debug, Clone)]
pub struct Kanban {
    image_name: String,
    image_tag: String,
    postgres_tag: String,
    nats_image_name: String,
    nats_tag: String,
    opensearch_tag: String,
    minio_tag: String,
    gateway_image_name: String,
    gateway_image_tag: String,
    extra_env: HashMap<String, String>,
}

impl Kanban {
    /// Default kanban server image name.
    pub const DEFAULT_IMAGE_NAME: &'static str = "ghcr.io/sunbeamdotpt/kanban";
    /// Default kanban server image tag.
    pub const DEFAULT_IMAGE_TAG: &'static str = "latest";

    /// Kanban server HTTP port inside the container (ConnectRPC + health).
    pub const PORT: u16 = 8080;

    /// NATS client port inside its container.
    pub const NATS_PORT: u16 = Nats::PORT;
    /// MinIO S3 API port inside its container.
    pub const MINIO_PORT: u16 = 9000;

    /// Create a new orchestrator with the default pre-built kanban image.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the kanban server image name and tag.
    pub fn with_image(mut self, name: impl Into<String>, tag: impl Into<String>) -> Self {
        self.image_name = name.into();
        self.image_tag = tag.into();
        self
    }

    /// Override the kanban server image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.image_tag = tag.into();
        self
    }

    /// Override the sso-gateway image name and tag.
    pub fn with_gateway_image(mut self, name: impl Into<String>, tag: impl Into<String>) -> Self {
        self.gateway_image_name = name.into();
        self.gateway_image_tag = tag.into();
        self
    }

    /// Override the Postgres image tag.
    pub fn with_postgres_tag(mut self, tag: impl Into<String>) -> Self {
        self.postgres_tag = tag.into();
        self
    }

    /// Override the NATS image name.
    pub fn with_nats_image(mut self, name: impl Into<String>) -> Self {
        self.nats_image_name = name.into();
        self
    }

    /// Override the NATS image tag.
    pub fn with_nats_tag(mut self, tag: impl Into<String>) -> Self {
        self.nats_tag = tag.into();
        self
    }

    /// Override the OpenSearch image tag.
    pub fn with_opensearch_tag(mut self, tag: impl Into<String>) -> Self {
        self.opensearch_tag = tag.into();
        self
    }

    /// Override the MinIO image tag.
    pub fn with_minio_tag(mut self, tag: impl Into<String>) -> Self {
        self.minio_tag = tag.into();
        self
    }

    /// Inject an extra environment variable into the kanban server container
    /// (e.g. `KANBAN_SYSTEM_TENANT_ID`).
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_env.insert(key.into(), value.into());
        self
    }

    /// Start the full stack and return a handle to the running server.
    pub async fn start(self) -> Result<KanbanHandle, testcontainers::TestcontainersError> {
        let prefix = unique_prefix();
        let network = format!("{prefix}-net");

        let postgres_name = format!("{prefix}-postgres");
        let nats_name = format!("{prefix}-nats");
        let opensearch_name = format!("{prefix}-opensearch");
        let minio_name = format!("{prefix}-minio");
        let server_name = format!("{prefix}-server");

        // ── 1. Backing services on the shared network ────────────────────
        let postgres = Postgres::new()
            .with_tag(&self.postgres_tag)
            .with_credentials("sunbeam", "sunbeam", "kanban")
            .with_network(&network)
            .with_container_name(&postgres_name)
            .start()
            .await?;

        let nats = Nats::new()
            .with_image(&self.nats_image_name)
            .with_tag(&self.nats_tag)
            .with_network(&network)
            .with_container_name(&nats_name)
            .start()
            .await?;

        // OpenSearch publishes its port so the test process can poll cluster
        // health: the container has no log-based readiness wait and the kanban
        // server crashes on its backfill migration when the REST API is not up
        // yet (it does not retry system migrations).
        let opensearch = OpenSearch::new()
            .with_tag(&self.opensearch_tag)
            .with_network(&network)
            .with_container_name(&opensearch_name)
            .publish_ports()
            .start()
            .await?;

        let opensearch_url = util::container_host_url(&opensearch, OpenSearch::REST_PORT)
            .await
            .map_err(testcontainers::TestcontainersError::other)?;
        wait_for_opensearch(&opensearch_url, 180)
            .await
            .map_err(testcontainers::TestcontainersError::other)?;

        // MinIO publishes its port so the test process can create the bucket.
        let minio = GenericImage::new("minio/minio", &self.minio_tag)
            .with_exposed_port(ContainerPort::Tcp(Self::MINIO_PORT))
            .with_mapped_port(0, ContainerPort::Tcp(Self::MINIO_PORT))
            .with_env_var("MINIO_ROOT_USER", MINIO_ROOT_USER)
            .with_env_var("MINIO_ROOT_PASSWORD", MINIO_ROOT_PASSWORD)
            .with_cmd(vec!["server", "/data"])
            .with_network(&network)
            .with_container_name(&minio_name)
            .with_startup_timeout(Duration::from_secs(120))
            .start()
            .await?;

        // ── 2. sso-gateway stack on the same network ─────────────────────
        let bootstrap_client_secret = format!("sunbeam-test-{prefix}-bootstrap-secret");
        let gateway = SsoGateway::new()
            .with_image(&self.gateway_image_name, &self.gateway_image_tag)
            .with_network(&network)
            .with_env("SYSTEM_BOOTSTRAP_CLIENT_SECRET", &bootstrap_client_secret)
            .start()
            .await?;

        // ── 3. Provision the service tenant + application ────────────────
        let (tenant_id, client_id, client_secret) =
            provision_service_app(gateway.endpoint(), &bootstrap_client_secret)
                .await
                .map_err(testcontainers::TestcontainersError::other)?;

        // ── 4. Create the attachments bucket ─────────────────────────────
        let minio_url = util::container_host_url(&minio, Self::MINIO_PORT)
            .await
            .map_err(testcontainers::TestcontainersError::other)?;
        wait_for_http_ok(&format!("{minio_url}/minio/health/live"), 60)
            .await
            .map_err(testcontainers::TestcontainersError::other)?;
        create_s3_bucket(&minio_url, S3_BUCKET, MINIO_ROOT_USER, MINIO_ROOT_PASSWORD)
            .await
            .map_err(testcontainers::TestcontainersError::other)?;

        // ── 5. Kanban server ─────────────────────────────────────────────
        let database_url = format!("postgres://sunbeam:sunbeam@{postgres_name}:5432/kanban");
        let mut image = GenericImage::new(&self.image_name, &self.image_tag)
            .with_exposed_port(ContainerPort::Tcp(Self::PORT))
            // No log-based wait: the readiness log line differs across
            // published image versions ("kanban listening" vs older builds).
            // The host-side /healthz/live poll below is version-proof.
            .with_mapped_port(0, ContainerPort::Tcp(Self::PORT))
            .with_container_name(&server_name)
            .with_network(&network)
            .with_env_var("DATABASE_URL", database_url)
            .with_env_var(
                "NATS_URL",
                format!("nats://{nats_name}:{}", Self::NATS_PORT),
            )
            .with_env_var(
                "SSO_GATEWAY_INTROSPECTION_URL",
                format!("{}/oauth2/introspect", gateway.internal_url()),
            )
            .with_env_var("SSO_GATEWAY_CLIENT_ID", &client_id)
            .with_env_var("SSO_GATEWAY_CLIENT_SECRET", &client_secret)
            .with_env_var(
                "OPENSEARCH_URL",
                format!("http://{opensearch_name}:{}", OpenSearch::REST_PORT),
            )
            .with_env_var(
                "S3_ENDPOINT",
                format!("http://{minio_name}:{}", Self::MINIO_PORT),
            )
            .with_env_var("S3_REGION", "us-east-1")
            .with_env_var("S3_ACCESS_KEY", MINIO_ROOT_USER)
            .with_env_var("S3_SECRET_KEY", MINIO_ROOT_PASSWORD)
            .with_env_var("S3_BUCKET", S3_BUCKET)
            .with_startup_timeout(Duration::from_secs(600));

        for (key, value) in &self.extra_env {
            image = image.with_env_var(key, value);
        }

        let server = image.start().await?;

        let endpoint = util::container_host_url(&server, Self::PORT)
            .await
            .map_err(testcontainers::TestcontainersError::other)?;
        wait_for_http_ok(&format!("{endpoint}/healthz/live"), 300)
            .await
            .map_err(testcontainers::TestcontainersError::other)?;

        Ok(KanbanHandle {
            endpoint,
            sso_gateway_endpoint: gateway.endpoint().to_owned(),
            tenant_id,
            client_id,
            client_secret,
            _postgres: postgres,
            _nats: nats,
            _opensearch: opensearch,
            _minio: minio,
            _gateway: gateway,
            server,
        })
    }
}

impl Default for Kanban {
    fn default() -> Self {
        Self {
            image_name: Self::DEFAULT_IMAGE_NAME.to_owned(),
            image_tag: Self::DEFAULT_IMAGE_TAG.to_owned(),
            postgres_tag: Postgres::DEFAULT_TAG.to_owned(),
            nats_image_name: Nats::NAME.to_owned(),
            nats_tag: Nats::DEFAULT_TAG.to_owned(),
            opensearch_tag: OpenSearch::DEFAULT_TAG.to_owned(),
            minio_tag: "RELEASE.2025-02-28T09-55-16Z".to_owned(),
            gateway_image_name: SsoGateway::DEFAULT_IMAGE_NAME.to_owned(),
            gateway_image_tag: SsoGateway::DEFAULT_IMAGE_TAG.to_owned(),
            extra_env: HashMap::new(),
        }
    }
}

/// A running kanban server stack.
///
/// Dropping this value stops and removes all containers.
pub struct KanbanHandle {
    endpoint: String,
    sso_gateway_endpoint: String,
    tenant_id: String,
    client_id: String,
    client_secret: String,
    #[allow(dead_code)]
    _postgres: ContainerAsync<GenericImage>,
    #[allow(dead_code)]
    _nats: ContainerAsync<GenericImage>,
    #[allow(dead_code)]
    _opensearch: ContainerAsync<GenericImage>,
    #[allow(dead_code)]
    _minio: ContainerAsync<GenericImage>,
    #[allow(dead_code)]
    _gateway: SsoGatewayHandle,
    server: ContainerAsync<GenericImage>,
}

impl KanbanHandle {
    /// Return the kanban server's public HTTP endpoint (ConnectRPC base URL).
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Return the sso-gateway endpoint, for minting user tokens in tests.
    pub fn sso_gateway_endpoint(&self) -> &str {
        &self.sso_gateway_endpoint
    }

    /// Return the provisioned `kanban-test` tenant id.
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    /// Return the provisioned service application's client id.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Return the provisioned service application's client secret.
    pub fn client_secret(&self) -> &str {
        &self.client_secret
    }

    /// Stop the server container and drop the stack.
    ///
    /// The containers are removed when the handle is dropped; this method is
    /// provided as an explicit hook for tests that want to await cleanup.
    pub async fn shutdown(self) {
        drop(self.server);
        // Give testcontainers a moment to schedule removals before returning.
        sleep(Duration::from_millis(100)).await;
    }
}

// ── IAM provisioning ────────────────────────────────────────────────────────

/// Exchange the system bootstrap client credentials for an access token with
/// the requested scope.
///
/// The gateway's OAuth2 token endpoint expects HTTP Basic authentication
/// (`client_secret_basic`) rather than form-encoded credentials.
async fn fetch_bootstrap_token(
    gateway_url: &str,
    bootstrap_secret: &str,
    scope: &str,
) -> Result<String, BoxError> {
    let resp: serde_json::Value = reqwest::Client::new()
        .post(format!("{gateway_url}/oauth2/token"))
        .basic_auth(SYSTEM_BOOTSTRAP_CLIENT_ID, Some(bootstrap_secret))
        .form(&[("grant_type", "client_credentials"), ("scope", scope)])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    resp.get("access_token")
        .and_then(|t| t.as_str())
        .map(str::to_owned)
        .ok_or_else(|| "token response has no access_token".into())
}

/// Build an IAM admin client authenticated with a bootstrap token.
fn admin_client(gateway_url: &str, token: String) -> Result<AuthClient, BoxError> {
    let g2v = AuthClient::builder(gateway_url)
        .auth(sunbeam_g2v::client::BearerToken::new(token))
        .build()?;
    let base_uri = gateway_url
        .parse()
        .map_err(|_| format!("invalid sso-gateway URL: {gateway_url}"))?;
    Ok(AuthClient::new(g2v, base_uri)?)
}

/// Create the `kanban-test` tenant and a `kanban-service` application with
/// `permission:admin` and `tenant:admin`, returning
/// `(tenant_id, client_id, client_secret)`.
///
/// Mirrors the kanban repo's own test harness: the application uses
/// `client_secret_post` so that `sunbeam_g2v::client::OAuth2ClientCredentials`
/// can fetch tokens with form-encoded credentials, and `cross_tenant: true`
/// so the server can act on behalf of other tenants via `x-tenant-id`.
async fn provision_service_app(
    gateway_url: &str,
    bootstrap_secret: &str,
) -> Result<(String, String, String), BoxError> {
    let tenant_token = fetch_bootstrap_token(gateway_url, bootstrap_secret, "tenant:admin").await?;
    let tenant = admin_client(gateway_url, tenant_token)?
        .tenant()
        .create_tenant(v1::CreateTenantRequest {
            slug: "kanban-test".to_owned(),
            display_name: "Kanban Test Tenant".to_owned(),
            settings: Default::default(),
            __buffa_unknown_fields: Default::default(),
        })
        .await
        .map_err(|e| format!("CreateTenant failed: {e}"))?
        .into_owned();

    let app_token =
        fetch_bootstrap_token(gateway_url, bootstrap_secret, "application:admin").await?;
    let app_client = admin_client(gateway_url, app_token)?.with_tenant(tenant.id.clone());

    // Hydra serializes OAuth2 client writes; parallel stacks can lose the race
    // with a transient "Unable to serialize access" conflict. Retry only the
    // app provisioning — the tenant already exists at this point.
    let mut attempt = 0u32;
    let secret = loop {
        attempt += 1;
        let created = async {
            let app = app_client
                .application()
                .create_application(v1::CreateApplicationRequest {
                    name: "kanban-service".to_owned(),
                    redirect_uris: vec![],
                    grant_types: vec!["client_credentials".to_owned()],
                    response_types: vec!["token".to_owned()],
                    // identity:admin lets tests provision directory users
                    // (e.g. assignee email resolution checks); identity:read
                    // is the scope the kanban server's own identity client
                    // requests when resolving assignee profiles.
                    scope: vec![
                        "permission:admin".to_owned(),
                        "tenant:admin".to_owned(),
                        "identity:admin".to_owned(),
                        "identity:read".to_owned(),
                    ],
                    token_endpoint_auth_method: "client_secret_post".to_owned(),
                    cross_tenant: true,
                    // Machine-to-machine client (client_credentials only);
                    // no browser authorization flow, so consent never applies.
                    skip_consent: false,
                    __buffa_unknown_fields: Default::default(),
                })
                .await
                .map_err(|e| format!("CreateApplication failed: {e}"))?
                .into_owned();

            app_client
                .application()
                .rotate_secret(v1::RotateSecretRequest {
                    id: app.id,
                    __buffa_unknown_fields: Default::default(),
                })
                .await
                .map_err(|e| format!("RotateSecret failed: {e}"))
                .map(|r| r.into_owned())
        }
        .await;

        match created {
            Ok(secret) => break secret,
            Err(e) => {
                let transient = e.contains("Unable to serialize access");
                if !transient || attempt >= 5 {
                    return Err(e.into());
                }
                sleep(Duration::from_millis(250 * u64::from(attempt))).await;
            }
        }
    };

    Ok((tenant.id, secret.client_id, secret.client_secret))
}

// ── MinIO bucket creation (AWS SigV4, no extra dependencies) ───────────────

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>, BoxError> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac =
        Hmac::<Sha256>::new_from_slice(key).map_err(|e| format!("hmac key rejected: {e}"))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Create an S3 bucket with a SigV4-signed `PUT /{bucket}` request.
///
/// MinIO does not auto-create buckets and the SDK deliberately has no S3
/// client dependency, so the single request is signed by hand with the
/// already-available `hmac`/`sha2` crates.
async fn create_s3_bucket(
    endpoint: &str,
    bucket: &str,
    access_key: &str,
    secret_key: &str,
) -> Result<(), BoxError> {
    use sha2::{Digest, Sha256};

    let host = endpoint
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    let now = chrono::Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = now.format("%Y%m%d").to_string();
    let region = "us-east-1";

    let canonical_request = format!(
        "PUT\n/{bucket}\n\nhost:{host}\nx-amz-content-sha256:UNSIGNED-PAYLOAD\nx-amz-date:{amz_date}\n\nhost;x-amz-content-sha256;x-amz-date\nUNSIGNED-PAYLOAD"
    );
    let canonical_hash = hex_lower(&Sha256::digest(canonical_request.as_bytes()));
    let scope = format!("{date}/{region}/s3/aws4_request");
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{canonical_hash}");

    let k_date = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), date.as_bytes())?;
    let k_region = hmac_sha256(&k_date, region.as_bytes())?;
    let k_service = hmac_sha256(&k_region, b"s3")?;
    let k_signing = hmac_sha256(&k_service, b"aws4_request")?;
    let signature = hex_lower(&hmac_sha256(&k_signing, string_to_sign.as_bytes())?);

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature={signature}"
    );

    let resp = reqwest::Client::new()
        .put(format!("{endpoint}/{bucket}"))
        .header("x-amz-date", amz_date)
        .header("x-amz-content-sha256", "UNSIGNED-PAYLOAD")
        .header("authorization", authorization)
        .body("")
        .send()
        .await?;

    // 200 = created, 409 BucketAlreadyOwnedByYou = already there (idempotent).
    if resp.status().is_success() || resp.status().as_u16() == 409 {
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(format!("create bucket {bucket} failed: {status} {body}").into())
    }
}

/// Poll an URL until it returns a 2xx response or the deadline passes.
async fn wait_for_http_ok(url: &str, timeout_secs: u64) -> Result<(), BoxError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            _ => {
                if Instant::now() >= deadline {
                    return Err(format!("no successful response from {url}").into());
                }
                sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

/// Poll the OpenSearch cluster health endpoint until it reports green or
/// yellow (a fresh single-node cluster goes yellow, never green).
async fn wait_for_opensearch(base_url: &str, timeout_secs: u64) -> Result<(), BoxError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let url = format!("{base_url}/_cluster/health");
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if let Ok(resp) = client.get(&url).send().await
            && resp.status().is_success()
            && let Ok(body) = resp.text().await
            && (body.contains("\"status\":\"green\"") || body.contains("\"status\":\"yellow\""))
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("opensearch did not become healthy at {url}").into());
        }
        sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(all(test, feature = "testing"))]
mod image_tests {
    use super::Kanban;

    #[tokio::test]
    #[ignore = "requires pre-built kanban + sso-gateway images and pulls five containers"]
    async fn kanban_stack_exposes_ready_endpoint() {
        let stack = Kanban::new()
            .start()
            .await
            .expect("kanban stack should start");

        let resp = reqwest::get(format!("{}/healthz/live", stack.endpoint()))
            .await
            .expect("kanban should accept request");

        assert!(
            resp.status().is_success(),
            "kanban /healthz/live should be successful: {}",
            resp.status()
        );
    }
}
