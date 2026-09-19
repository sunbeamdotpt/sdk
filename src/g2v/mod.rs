//! Sunbeam Service Framework — the g2v framework, vendored into the sdk.
//!
//! Vendored from [`sunbeamdotpt/g2v`](https://github.com/sunbeamdotpt/g2v)
//! at v0.6.2 (final standalone release); that repo is deprecated in favor of
//! this module. Client and server stacks are independently feature-gated:
//! `g2v-client` (outbound HTTP/ConnectRPC client stack) and `g2v-server`
//! (axum service runtime, middleware, metrics, telemetry, health). The
//! `g2v` feature enables both; granular `g2v-*` features opt into NATS,
//! sqlx, redis, vault/election, and the standalone hyper server.

#![allow(refining_impl_trait_internal, refining_impl_trait_reachable)]

#[cfg(feature = "g2v-server")]
pub mod config;
pub mod error;
#[cfg(feature = "g2v-server")]
pub mod health;
#[cfg(feature = "g2v-server")]
pub mod metrics;
pub mod middleware;
#[cfg(feature = "g2v-nats")]
mod nats_util;
pub mod prelude;
#[cfg(feature = "g2v-server")]
pub mod router;
#[cfg(feature = "g2v-server")]
pub mod service;
#[cfg(feature = "g2v-server")]
pub mod telemetry;
#[cfg(feature = "g2v-server")]
pub mod testing;

#[cfg(feature = "g2v-server")]
pub use health::HealthRouter;

pub mod client;
#[cfg(feature = "g2v-sqlx")]
pub mod db;
#[cfg(feature = "g2v-election")]
pub mod election;
#[cfg(feature = "g2v-nats")]
pub mod mq;
pub mod server;

// Re-export connectrpc essentials for convenience
#[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
pub use connectrpc::{
    ConnectError, ConnectRpcBody, ConnectRpcService, ErrorCode, Protocol, RequestContext, Router,
    Server,
};

// Re-export buffa essentials
#[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
pub use buffa::view::{MessageView, OwnedView};

/// Boxed error type for middleware
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Boxed future type
pub type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

/// Result type using BoxError
pub type BoxResult<T> = Result<T, BoxError>;

#[cfg(test)]
mod tests {
    #[test]
    fn test_lib_compiles() {
        // This test just verifies the library compiles
        // Real tests are in the tests/ directory
    }
}
