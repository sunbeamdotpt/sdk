//! Prelude module for convenient wildcard imports.
//!
//! ```rust
//! use crate::g2v::prelude::*;
//! ```

#[cfg(feature = "g2v-client-connectrpc")]
pub use crate::g2v::client::ConnectTransport;
#[cfg(feature = "g2v-cache")]
pub use crate::g2v::client::cache::{ClientCacheLayer, ClientCachePredicate, ClientCacheService};
#[cfg(feature = "g2v-client")]
pub use crate::g2v::client::{
    BearerToken, Client, ClientBuilder, ClientBuilderError, ClientError, GraphqlClient,
    OAuth2ClientCredentials, RestClient, TokenProvider,
};
#[cfg(feature = "g2v-client")]
pub use crate::g2v::client::{
    SafeDnsResolver, is_forbidden_ip, safe_reqwest_client, validate_resolved_ips_for_url,
    validate_upstream_url,
};
#[cfg(feature = "g2v-server")]
pub use crate::g2v::config::AuthConfig;
#[cfg(feature = "g2v-server")]
pub use crate::g2v::config::{
    ConfigError, DatabaseConfig, ElectionConfig, LimitsConfig, NatsConfig, ObservabilityConfig,
    ServiceConfig, VaultConfig,
};
pub use crate::g2v::error::{ServiceError, ServiceResult};
#[cfg(feature = "g2v-server")]
pub use crate::g2v::middleware::audit::audit_middleware;
#[cfg(feature = "g2v-server")]
pub use crate::g2v::middleware::auth::{
    AuthContext, AuthMiddlewareState, CookieSigner, SessionClaims, SessionStore, SessionTokenError,
    SessionTokenSigner, TenantId,
};
#[cfg(feature = "g2v-server")]
pub use crate::g2v::middleware::auth::{
    CachedIntrospectionSessionClient, IntrospectionConfig, IntrospectionSessionClient,
};
#[cfg(all(feature = "g2v-cache", feature = "g2v-server"))]
pub use crate::g2v::middleware::cache::{CacheConfig, CacheLayer, CacheScope};
#[cfg(feature = "g2v-server")]
pub use crate::g2v::middleware::rate_limit::{RateLimiter, rate_limit_middleware};
#[cfg(feature = "g2v-server")]
pub use crate::g2v::router::ServiceRouter;
#[cfg(feature = "g2v-server")]
pub use crate::g2v::server::ServerConfig;
#[cfg(feature = "g2v-server")]
pub use crate::g2v::server::{axum::AxumServer, builder::ServerBuilder};
#[cfg(feature = "g2v-server")]
pub use crate::g2v::service::{ServiceExt, SunbeamService};
#[cfg(feature = "g2v-server")]
pub use crate::g2v::telemetry::{TelemetryConfig, TelemetryGuard};

#[cfg(feature = "g2v-sqlx")]
pub use crate::g2v::db::Database;
#[cfg(feature = "g2v-sqlx")]
pub use crate::g2v::db::crypto::{decrypt, encrypt};
#[cfg(feature = "g2v-election")]
pub use crate::g2v::election::{ElectionError, LeaderElection, LeaderHandle};
#[cfg(feature = "g2v-server")]
pub use crate::g2v::testing::TestHarness;

// Re-export connectrpc essentials
#[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
pub use buffa::view::{MessageView, OwnedView};
#[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
pub use connectrpc::{ConnectError, ErrorCode, RequestContext, Router};

// Re-export common types
pub use http::{HeaderMap, HeaderName, HeaderValue};
pub use std::sync::Arc;
pub use std::time::Duration;
pub use tower::{Layer, Service};

/// Common result type for fallible operations in Sunbeam services.
///
/// Defaults to [`ServiceError`] as the error type.
pub type Result<T, E = ServiceError> = std::result::Result<T, E>;
