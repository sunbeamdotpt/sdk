//! Infrastructure API — SDK client for the Sunbeam iapi service.
//!
//! The entire `sunbeam.iapi.v1` surface is generated from the protos
//! vendored under `proto/sunbeam/iapi` (the iapi service keeps them as a
//! local buf module, not on the BSR) and exposed under [`v1`].
//! [`IapiClient`] wraps a [`crate::g2v::client::Client`] to provide
//! ready-to-use service clients speaking ConnectRPC.

#![allow(missing_docs)]

// Generated stubs are tool output (connectrpc-build / buffa view codegen
// emits `unwrap_or_default` internally); exempt from the fleet-wide
// *_or_default ban, which targets hand-written code.
#[allow(clippy::disallowed_methods)]
mod generated {
    connectrpc::include_generated!("iapi/_connectrpc.rs");
}
pub use generated::*;

pub use crate::iapi::sunbeam::iapi::v1;

use crate::g2v::client::{
    Client as G2vClient, ClientBuilder, ClientBuilderError, ConnectTransport,
};
use connectrpc::client::ClientConfig;
use std::sync::Arc;

/// Errors that can occur when constructing or using an [`IapiClient`].
#[derive(Debug, thiserror::Error)]
pub enum IapiClientError {
    /// The supplied base URL could not be parsed.
    #[error("invalid iapi server URL: {0}")]
    InvalidUrl(String),
    /// The underlying HTTP client could not be constructed.
    #[error("failed to build iapi client: {0}")]
    Build(#[from] ClientBuilderError),
}

/// SDK client for the Sunbeam Infrastructure API surface.
///
/// `IapiClient` is cheap to clone: it holds an [`Arc`] around the configured
/// g2v HTTP client stack.
#[derive(Clone, Debug)]
pub struct IapiClient {
    client: Arc<G2vClient>,
    base_uri: http::Uri,
    config: ClientConfig,
}

impl IapiClient {
    /// Create a builder for an iapi client rooted at the given server URL.
    ///
    /// The URL should be the base of the iapi deployment, e.g.
    /// `https://iapi.example.com`.
    pub fn builder(base_url: impl Into<String>) -> ClientBuilder {
        ClientBuilder::new(base_url)
    }

    /// Create an unauthenticated client rooted at the given server URL.
    ///
    /// Convenience for the common `builder(url).build()` +
    /// `new(client, url.parse()?)` pair. Use [`builder`](Self::builder)
    /// instead when authentication or other g2v options are needed.
    pub fn connect(base_url: impl Into<String>) -> Result<Self, IapiClientError> {
        let base_url = base_url.into();
        let base_uri: http::Uri = base_url
            .parse()
            .map_err(|_| IapiClientError::InvalidUrl(base_url.clone()))?;
        let client = ClientBuilder::new(base_url).build()?;
        Self::new(client, base_uri)
    }

    /// Create a client from an existing g2v client and base URI.
    ///
    /// # Errors
    ///
    /// Returns [`IapiClientError::InvalidUrl`] when `base_uri` cannot be used
    /// to build a ConnectRPC client configuration.
    pub fn new(client: G2vClient, base_uri: http::Uri) -> Result<Self, IapiClientError> {
        let config = ClientConfig::new(base_uri.clone());
        Ok(Self {
            client: Arc::new(client),
            base_uri,
            config,
        })
    }

    /// Return the configured base URI.
    pub fn base_uri(&self) -> &http::Uri {
        &self.base_uri
    }

    /// Return a copy of this client that sends a default header on every
    /// request.
    ///
    /// The header is set as a default on the underlying
    /// [`connectrpc::client::ClientConfig`]; per-call
    /// [`connectrpc::client::CallOptions`] headers with the same name take
    /// precedence over it.
    #[must_use]
    pub fn with_default_header(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.config = self.config.with_default_header(name.into(), value.into());
        self
    }

    fn transport(&self) -> ConnectTransport {
        self.client.connectrpc(self.base_uri.clone())
    }

    /// Client for region management (fabric onboarding, lifecycle).
    pub fn regions(&self) -> v1::RegionServiceClient<ConnectTransport> {
        v1::RegionServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for networks, subnets, and VPC fabric.
    pub fn networks(&self) -> v1::NetworkServiceClient<ConnectTransport> {
        v1::NetworkServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for virtual machine provisioning and lifecycle (W1).
    pub fn vms(&self) -> v1::VirtualMachineServiceClient<ConnectTransport> {
        v1::VirtualMachineServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for virtual disk management.
    pub fn disks(&self) -> v1::DiskServiceClient<ConnectTransport> {
        v1::DiskServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for image catalog management.
    pub fn images(&self) -> v1::ImageServiceClient<ConnectTransport> {
        v1::ImageServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for VM/cluster blueprints.
    pub fn blueprints(&self) -> v1::BlueprintServiceClient<ConnectTransport> {
        v1::BlueprintServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for downstream cluster provisioning (W2, Rancher).
    pub fn clusters(&self) -> v1::ClusterServiceClient<ConnectTransport> {
        v1::ClusterServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for the CIDR ledger (row allocation across regions).
    pub fn ledger(&self) -> v1::LedgerServiceClient<ConnectTransport> {
        v1::LedgerServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for DNS zone and record management (PDNS churn).
    pub fn dns(&self) -> v1::DnsServiceClient<ConnectTransport> {
        v1::DnsServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for the hash-chained compliance/audit trail.
    pub fn audit(&self) -> v1::AuditServiceClient<ConnectTransport> {
        v1::AuditServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for crash/rebuild rehydration of domain state from the planes.
    pub fn rehydrate(&self) -> v1::RehydrateServiceClient<ConnectTransport> {
        v1::RehydrateServiceClient::new(self.transport(), self.config.clone())
    }
}

/// Re-exports of the crates that appear in the generated iapi API surface.
///
/// The generated clients expose types from `connectrpc`, `buffa`,
/// `buffa-types`, and `sunbeam-g2v` (e.g. `CallOptions`, `ConnectError`,
/// `MessageField`, `BearerToken`). Name them through this prelude instead of
/// adding direct dependencies so the versions always match the ones the SDK
/// was compiled against.
pub mod prelude {
    pub use super::{IapiClient, IapiClientError, v1};
    pub use crate::g2v;
    pub use buffa;
    pub use buffa_types;
    pub use connectrpc;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iapi_client_builder_creates_g2v_builder() {
        let builder = IapiClient::builder("https://iapi.example.com");
        let _ = builder;
    }

    #[test]
    fn test_iapi_client_new_roundtrip() {
        let g2v = IapiClient::builder("https://iapi.example.com")
            .build()
            .unwrap();
        let client = IapiClient::new(g2v, "https://iapi.example.com".parse().unwrap()).unwrap();
        assert_eq!(client.base_uri().to_string(), "https://iapi.example.com/");
    }

    #[test]
    fn test_iapi_client_connect_roundtrip() {
        let client = IapiClient::connect("https://iapi.example.com").unwrap();
        assert_eq!(client.base_uri().to_string(), "https://iapi.example.com/");
    }

    #[test]
    fn test_iapi_client_connect_rejects_invalid_url() {
        let err = IapiClient::connect("not a url").unwrap_err();
        assert!(matches!(err, IapiClientError::InvalidUrl(_)));
    }

    #[test]
    fn test_iapi_client_with_default_header() {
        let client = IapiClient::connect("https://iapi.example.com")
            .unwrap()
            .with_default_header("x-sunbeam-region", "cdg");
        assert_eq!(
            client.config.default_headers().get("x-sunbeam-region"),
            Some(&http::HeaderValue::from_static("cdg"))
        );
    }

    // End-to-end decode through the real client stack: GetRegion returns the
    // resource directly (house style — no response wrapper), proto codec.
    #[tokio::test]
    async fn test_get_region_decodes_proto_response() {
        use buffa::Message as _;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let response = v1::Region {
            code: "cdg".into(),
            cidr_block: "10.64.0.0/14".into(),
            ..Default::default()
        };
        Mock::given(method("POST"))
            .and(path("/sunbeam.iapi.v1.RegionService/GetRegion"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/proto")
                    .set_body_bytes(response.encode_to_bytes()),
            )
            .mount(&server)
            .await;

        let client = IapiClient::connect(server.uri()).unwrap();
        let region = client
            .regions()
            .get_region(v1::GetRegionRequest {
                code: "cdg".into(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_owned();

        assert_eq!(region.code, "cdg");
        assert_eq!(region.cidr_block, "10.64.0.0/14");
    }
}
