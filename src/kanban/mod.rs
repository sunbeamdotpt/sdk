//! Kanban project management — SDK client for the Sunbeam kanban service.
//!
//! The entire kanban surface area is generated from `buf.build/sunbeamdotpt/kanban`
//! and exposed under [`v1`]. [`KanbanClient`] wraps a
//! [`sunbeam_g2v::client::Client`] to provide ready-to-use service clients
//! speaking ConnectRPC.

#![allow(missing_docs)]

connectrpc::include_generated!("kanban/_connectrpc.rs");

pub use crate::kanban::sunbeam::kanban::v1;

use connectrpc::client::ClientConfig;
use std::sync::Arc;
use sunbeam_g2v::client::{
    Client as G2vClient, ClientBuilder, ClientBuilderError, ConnectTransport,
};

/// Errors that can occur when constructing or using a [`KanbanClient`].
#[derive(Debug, thiserror::Error)]
pub enum KanbanClientError {
    /// The supplied base URL could not be parsed.
    #[error("invalid kanban server URL: {0}")]
    InvalidUrl(String),
    /// The underlying HTTP client could not be constructed.
    #[error("failed to build kanban client: {0}")]
    Build(#[from] ClientBuilderError),
}

/// SDK client for the Sunbeam kanban service surface.
///
/// `KanbanClient` is cheap to clone: it holds an [`Arc`] around the configured
/// g2v HTTP client stack.
#[derive(Clone, Debug)]
pub struct KanbanClient {
    client: Arc<G2vClient>,
    base_uri: http::Uri,
    config: ClientConfig,
}

impl KanbanClient {
    /// Create a builder for a kanban client rooted at the given server URL.
    ///
    /// The URL should be the base of the kanban deployment, e.g.
    /// `https://kanban.example.com`.
    pub fn builder(base_url: impl Into<String>) -> ClientBuilder {
        ClientBuilder::new(base_url)
    }

    /// Create an unauthenticated client rooted at the given server URL.
    ///
    /// Convenience for the common `builder(url).build()` +
    /// `new(client, url.parse()?)` pair. Use [`builder`](Self::builder) instead
    /// when authentication or other g2v options are needed.
    pub fn connect(base_url: impl Into<String>) -> Result<Self, KanbanClientError> {
        let base_url = base_url.into();
        let base_uri: http::Uri = base_url
            .parse()
            .map_err(|_| KanbanClientError::InvalidUrl(base_url.clone()))?;
        let client = ClientBuilder::new(base_url).build()?;
        Self::new(client, base_uri)
    }

    /// Create a client from an existing g2v client and base URI.
    ///
    /// # Errors
    ///
    /// Returns [`KanbanClientError::InvalidUrl`] when `base_uri` cannot be used
    /// to build a ConnectRPC client configuration.
    pub fn new(client: G2vClient, base_uri: http::Uri) -> Result<Self, KanbanClientError> {
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
    /// request (e.g. `x-sunbeam-object-id`).
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

    /// Client for board management.
    pub fn boards(&self) -> v1::BoardServiceClient<ConnectTransport> {
        v1::BoardServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for card management.
    pub fn cards(&self) -> v1::CardServiceClient<ConnectTransport> {
        v1::CardServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for project management.
    pub fn projects(&self) -> v1::ProjectServiceClient<ConnectTransport> {
        v1::ProjectServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for aggregated boards.
    pub fn aggregated_boards(&self) -> v1::AggregatedBoardServiceClient<ConnectTransport> {
        v1::AggregatedBoardServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for attachments.
    pub fn attachments(&self) -> v1::AttachmentServiceClient<ConnectTransport> {
        v1::AttachmentServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for GitHub issue links.
    pub fn github_links(&self) -> v1::GithubLinkServiceClient<ConnectTransport> {
        v1::GithubLinkServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for public boards.
    pub fn public_boards(&self) -> v1::PublicBoardServiceClient<ConnectTransport> {
        v1::PublicBoardServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for search.
    pub fn search(&self) -> v1::SearchServiceClient<ConnectTransport> {
        v1::SearchServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for board and card templates.
    pub fn templates(&self) -> v1::TemplatesServiceClient<ConnectTransport> {
        v1::TemplatesServiceClient::new(self.transport(), self.config.clone())
    }
}

/// Re-exports of the crates that appear in the generated kanban API surface.
///
/// The generated clients expose types from `connectrpc`, `buffa`,
/// `buffa-types`, and `sunbeam-g2v` (e.g. `CallOptions`, `ConnectError`,
/// `MessageField`, `FieldMask`, `BearerToken`). Name them through this
/// prelude instead of adding direct dependencies so the versions always match
/// the ones the SDK was compiled against.
pub mod prelude {
    pub use super::{KanbanClient, KanbanClientError, v1};
    pub use buffa;
    pub use buffa_types;
    pub use connectrpc;
    pub use sunbeam_g2v;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kanban_client_builder_creates_g2v_builder() {
        let builder = KanbanClient::builder("https://kanban.example.com");
        let _ = builder;
    }

    #[test]
    fn test_kanban_client_new_roundtrip() {
        let g2v = KanbanClient::builder("https://kanban.example.com")
            .build()
            .unwrap();
        let client = KanbanClient::new(g2v, "https://kanban.example.com".parse().unwrap()).unwrap();
        assert_eq!(client.base_uri().to_string(), "https://kanban.example.com/");
    }

    #[test]
    fn test_kanban_client_connect_roundtrip() {
        let client = KanbanClient::connect("https://kanban.example.com").unwrap();
        assert_eq!(client.base_uri().to_string(), "https://kanban.example.com/");
    }

    #[test]
    fn test_kanban_client_connect_rejects_invalid_url() {
        let err = KanbanClient::connect("not a url").unwrap_err();
        assert!(matches!(err, KanbanClientError::InvalidUrl(_)));
    }

    #[test]
    fn test_kanban_client_with_default_header() {
        let client = KanbanClient::connect("https://kanban.example.com")
            .unwrap()
            .with_default_header("x-sunbeam-object-id", "board-123");
        assert_eq!(
            client.config.default_headers().get("x-sunbeam-object-id"),
            Some(&http::HeaderValue::from_static("board-123"))
        );
    }
}
