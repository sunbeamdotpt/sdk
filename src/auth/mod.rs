//! Authentication and identity — SDK client for the Sunbeam sso-gateway.
//!
//! The entire sso-gateway surface area is generated from
//! `buf.build/sunbeamdotpt/sso-gateway` and exposed under [`iam::v1`].
//! [`AuthClient`] wraps a [`sunbeam_g2v::client::Client`] to provide ready-to-use
//! service clients speaking ConnectRPC.

#![allow(missing_docs)]

connectrpc::include_generated!("sso-gateway/_connectrpc.rs");

pub use crate::auth::iam::v1;

use connectrpc::client::ClientConfig;
use std::sync::Arc;
use sunbeam_g2v::client::{
    Client as G2vClient, ClientBuilder, ClientBuilderError, ConnectTransport,
};

/// Errors that can occur when constructing or using an [`AuthClient`].
#[derive(Debug, thiserror::Error)]
pub enum AuthClientError {
    /// The supplied base URL could not be parsed.
    #[error("invalid auth gateway URL: {0}")]
    InvalidUrl(String),
    /// The underlying HTTP client could not be constructed.
    #[error("failed to build auth client: {0}")]
    Build(#[from] ClientBuilderError),
}

/// SDK client for the Sunbeam sso-gateway IAM surface.
///
/// `AuthClient` is cheap to clone: it holds an [`Arc`] around the configured
/// g2v HTTP client stack.
#[derive(Clone, Debug)]
pub struct AuthClient {
    client: Arc<G2vClient>,
    base_uri: http::Uri,
    config: ClientConfig,
}

impl AuthClient {
    /// Create a builder for an auth client rooted at the given gateway URL.
    ///
    /// The URL should be the base of the sso-gateway deployment, e.g.
    /// `https://iam.example.com`.
    pub fn builder(base_url: impl Into<String>) -> ClientBuilder {
        ClientBuilder::new(base_url)
    }

    /// Create a client from an existing g2v client and base URI.
    ///
    /// # Errors
    ///
    /// Returns [`AuthClientError::InvalidUrl`] when `base_uri` cannot be used
    /// to build a ConnectRPC client configuration.
    pub fn new(client: G2vClient, base_uri: http::Uri) -> Result<Self, AuthClientError> {
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

    fn transport(&self) -> ConnectTransport {
        self.client.connectrpc(self.base_uri.clone())
    }

    /// Client for tenant management.
    pub fn tenant(&self) -> v1::TenantServiceClient<ConnectTransport> {
        v1::TenantServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for identity and session management.
    pub fn identity(&self) -> v1::IdentityServiceClient<ConnectTransport> {
        v1::IdentityServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for browser-facing self-service flows (login, registration,
    /// recovery, verification).
    pub fn identity_self_service(&self) -> v1::IdentitySelfServiceClient<ConnectTransport> {
        v1::IdentitySelfServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for OAuth2 device authorization grants.
    pub fn oauth2_device(&self) -> v1::OAuth2DeviceServiceClient<ConnectTransport> {
        v1::OAuth2DeviceServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for OAuth2 consent and logout request handling.
    pub fn oauth2_consent(&self) -> v1::OAuth2ConsentServiceClient<ConnectTransport> {
        v1::OAuth2ConsentServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for OIDC discovery, JWKS, and upstream federation flows.
    pub fn federation(&self) -> v1::FederationServiceClient<ConnectTransport> {
        v1::FederationServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for permission / relation tuple management.
    pub fn permission(&self) -> v1::PermissionServiceClient<ConnectTransport> {
        v1::PermissionServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for SCIM user and group provisioning.
    pub fn scim(&self) -> v1::ScimServiceClient<ConnectTransport> {
        v1::ScimServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for OAuth2/OIDC application registration.
    pub fn application(&self) -> v1::ApplicationServiceClient<ConnectTransport> {
        v1::ApplicationServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for machine-to-machine client credential management.
    pub fn client_credentials(&self) -> v1::ClientCredentialServiceClient<ConnectTransport> {
        v1::ClientCredentialServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for agent identity, delegation, and act-token management.
    pub fn agent(&self) -> v1::AgentServiceClient<ConnectTransport> {
        v1::AgentServiceClient::new(self.transport(), self.config.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_client_builder_creates_g2v_builder() {
        let builder = AuthClient::builder("https://iam.example.com");
        let _ = builder;
    }

    #[test]
    fn test_auth_client_new_roundtrip() {
        let g2v = AuthClient::builder("https://iam.example.com")
            .build()
            .unwrap();
        let client = AuthClient::new(g2v, "https://iam.example.com".parse().unwrap()).unwrap();
        assert_eq!(client.base_uri().to_string(), "https://iam.example.com/");
    }

    #[test]
    fn test_auth_client_client_credentials_accessor() {
        let g2v = AuthClient::builder("https://iam.example.com")
            .build()
            .unwrap();
        let client = AuthClient::new(g2v, "https://iam.example.com".parse().unwrap()).unwrap();
        let _ = client.client_credentials();
    }

    #[test]
    fn test_auth_client_agent_accessor() {
        let g2v = AuthClient::builder("https://iam.example.com")
            .build()
            .unwrap();
        let client = AuthClient::new(g2v, "https://iam.example.com".parse().unwrap()).unwrap();
        let _ = client.agent();
    }
}
