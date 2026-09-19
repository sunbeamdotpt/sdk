//! Client utilities for Sunbeam services.

#[cfg(feature = "g2v-client")]
pub mod auth;
#[cfg(feature = "g2v-client")]
pub mod builder;
#[cfg(feature = "g2v-cache")]
pub mod cache;
#[cfg(feature = "g2v-client-connectrpc")]
pub mod connect;
#[cfg(feature = "g2v-client")]
pub mod graphql;
#[cfg(feature = "g2v-client")]
pub mod propagation;
#[cfg(feature = "g2v-client")]
pub mod rest;
#[cfg(feature = "g2v-client")]
pub mod ssrf;
#[cfg(feature = "g2v-client")]
pub mod transport;

pub mod circuit_breaker;
pub mod factory;
pub mod retry;

#[cfg(feature = "g2v-client")]
pub use auth::{
    AuthLayer, AuthService, BearerToken, OAuth2ClientCredentials, TokenNotRefreshableError,
    TokenProvider,
};
#[cfg(feature = "g2v-client")]
pub use builder::{Client, ClientBuilder, ClientBuilderError};
#[cfg(feature = "g2v-cache")]
pub use cache::{ClientCacheLayer, ClientCachePredicate, ClientCacheService};
#[cfg(feature = "g2v-client")]
pub use graphql::{GraphqlClient, GraphqlError, GraphqlErrorLocation, GraphqlResponse};
#[cfg(feature = "g2v-client")]
pub use rest::{ClientError, RequestBuilder as RestRequestBuilder, RestClient};
#[cfg(feature = "g2v-client")]
pub use ssrf::{
    SafeDnsResolver, is_forbidden_ip, safe_reqwest_client, validate_resolved_ips_for_url,
    validate_upstream_url,
};
#[cfg(feature = "g2v-client")]
pub use transport::{ReqwestLayer, ReqwestService};

pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerLayer};
pub use retry::{RetryLayer, RetryPolicy};

#[cfg(feature = "g2v-client-connectrpc")]
pub use connect::ConnectTransport;
