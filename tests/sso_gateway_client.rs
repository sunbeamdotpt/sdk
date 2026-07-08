//! End-to-end integration tests for the sso-gateway [`AuthClient`].
//!
//! These tests boot the real sso-gateway reference stack in Docker via
//! `sunbeam_test::SsoGateway` and exercise the generated ConnectRPC clients
//! against it.
//!
//! They are serialized with a global mutex because each test starts a full
//! stack (Postgres, Redis, Hydra, Kratos, Keto, sso-gateway).

use std::time::Duration;

use connectrpc::client::CallOptions;
use sdk::auth::{AuthClient, v1};
use tokio::sync::Mutex;

mod support;
use support::SsoGateway;

const TENANT_ID_HEADER: &str = "x-tenant-id";
const SYSTEM_TENANT_ULID: &str = "01HZY9JTKKHK3Y6XJJYHZ9Q5TV";
const BOOTSTRAP_CLIENT_ID: &str = "system-bootstrap-client";
const BOOTSTRAP_CLIENT_SECRET: &str = "sunbeam-test-bootstrap-secret";

static STACK_LOCK: Mutex<()> = Mutex::const_new(());

async fn start_stack() -> (String, sunbeam_test::sso_gateway::SsoGatewayHandle) {
    support::init_docker_host();

    let gateway = SsoGateway::new()
        .with_image(
            sunbeam_test::sso_gateway::SsoGateway::DEFAULT_IMAGE_NAME,
            "v1.0.0-rc14",
        )
        .with_env("SYSTEM_BOOTSTRAP_CLIENT_SECRET", BOOTSTRAP_CLIENT_SECRET)
        .start()
        .await
        .expect("failed to start sso-gateway stack");

    let endpoint = gateway.endpoint().to_string();
    (endpoint, gateway)
}

/// Fetch an access token for the system bootstrap client using the OAuth2
/// client-credentials grant against the gateway's public token endpoint.
async fn bootstrap_access_token(endpoint: &str) -> String {
    // Hydra may take a moment longer than the gateway readiness probe.
    tokio::time::sleep(Duration::from_secs(3)).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{endpoint}/oauth2/token"))
        .basic_auth(BOOTSTRAP_CLIENT_ID, Some(BOOTSTRAP_CLIENT_SECRET))
        .form(&[("grant_type", "client_credentials")])
        .send()
        .await
        .expect("token request should complete");

    let status = resp.status();
    let body = resp
        .text()
        .await
        .expect("token response should have a body");
    assert!(status.is_success(), "token request failed: {status} {body}");

    let json: serde_json::Value =
        serde_json::from_str(&body).expect("token response should be json");
    json["access_token"]
        .as_str()
        .expect("access_token should be present")
        .to_string()
}

fn authenticated_options(token: &str) -> CallOptions {
    CallOptions::default()
        .with_header(TENANT_ID_HEADER, SYSTEM_TENANT_ULID)
        .with_header("authorization", format!("Bearer {token}"))
}

async fn auth_client(endpoint: &str) -> AuthClient {
    let g2v = AuthClient::builder(endpoint)
        .build()
        .expect("failed to build g2v client");
    AuthClient::new(g2v, endpoint.parse().expect("invalid sso-gateway URL"))
        .expect("failed to construct AuthClient")
}

/// Boot the stack and verify the federation discovery endpoint returns a
/// valid OpenID configuration for the system tenant.
#[tokio::test]
async fn sso_gateway_federation_openid_configuration() {
    let _guard = STACK_LOCK.lock().await;

    let (endpoint, _gateway) = start_stack().await;
    let client = auth_client(&endpoint).await;
    let token = bootstrap_access_token(&endpoint).await;

    let response = client
        .federation()
        .get_open_id_configuration_with_options(
            v1::GetOpenIDConfigurationRequest::default(),
            authenticated_options(&token),
        )
        .await
        .expect("GetOpenIDConfiguration should succeed for system tenant");

    let issuer = response.view().issuer;
    assert!(
        !issuer.is_empty(),
        "issuer should be present in OIDC configuration"
    );
}

/// The tenant service requires a token with `tenant:read` or `tenant:admin`
/// scope. The system bootstrap client currently issues tokens without that
/// scope, so this test is ignored until the test harness can provision a
/// suitably-scoped client.
#[tokio::test]
#[ignore = "requires a token with tenant:read/tenant:admin scope"]
async fn sso_gateway_tenant_list_tenants() {
    let _guard = STACK_LOCK.lock().await;

    let (endpoint, _gateway) = start_stack().await;
    let client = auth_client(&endpoint).await;
    let token = bootstrap_access_token(&endpoint).await;

    let mut request = v1::ListTenantsRequest::default();
    request.page.get_or_insert_default().page_size = 10;

    let response = client
        .tenant()
        .list_tenants_with_options(request, authenticated_options(&token))
        .await
        .expect("ListTenants should succeed for system tenant");

    let total = response.view().page.total_size;
    assert!(total > 0, "at least the system tenant should be returned");
}
