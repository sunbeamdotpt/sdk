//! Axum-based server implementation.
//!
//! Wraps a [`ServiceRouter`] into an `axum::Router` (via connectrpc's axum
//! integration) and provides `serve` / `serve_with_listener` that actually
//! bind a TCP listener and run the server.

use std::net::SocketAddr;

use axum::Router as AxumRouter;
use tokio::net::TcpListener;
use tower_http::catch_panic::CatchPanicLayer;

use crate::g2v::error::{ServiceError, ServiceResult};
#[cfg(feature = "g2v-cache")]
use crate::g2v::middleware::cache::{CacheConfig, CacheLayer};
use crate::g2v::middleware::request_id::RequestIdLayer;
#[cfg(feature = "g2v-server")]
use crate::g2v::middleware::tracing::TracingLayer;
use crate::g2v::router::ServiceRouter;

use super::ServerConfig;

/// Apply the built-in layers to a finished router.
///
/// `RequestIdLayer` honors or generates `x-request-id` and echoes it on the
/// response; `TracingLayer` (feature `tracing`) wraps everything in a server
/// span with W3C trace-context extraction; `CatchPanicLayer` sits outermost
/// so a panicking handler yields a 500 instead of dropping the connection.
fn with_builtin_layers(app: AxumRouter) -> AxumRouter {
    let app = app.layer(RequestIdLayer);
    #[cfg(feature = "g2v-server")]
    let app = app.layer(TracingLayer);
    app.layer(CatchPanicLayer::new())
}

/// Axum server.
///
/// Owns an `axum::Router` built from a `ServiceRouter` plus any user-supplied
/// extra routes (typically `/health`, `/metrics`).
pub struct AxumServer {
    app: AxumRouter,
    config: ServerConfig,
}

impl AxumServer {
    /// Build an Axum server from a `ServiceRouter` with default config.
    pub fn new(router: ServiceRouter) -> Self {
        Self::with_config(router, ServerConfig::default())
    }

    /// Build an Axum server with custom server config.
    pub fn with_config(router: ServiceRouter, config: ServerConfig) -> Self {
        let connect_router = router.into_inner();
        let app = connect_router.into_axum_router();
        Self { app, config }
    }

    /// Replace the inner axum router (e.g. to merge `/health`, `/metrics`).
    pub fn with_app(mut self, app: AxumRouter) -> Self {
        self.app = app;
        self
    }

    /// Merge extra routes (health, metrics, etc.) into the existing router.
    pub fn merge(mut self, other: AxumRouter) -> Self {
        self.app = self.app.merge(other);
        self
    }

    /// Enable the response cache on this server.
    #[cfg(feature = "g2v-cache")]
    pub fn with_cache(mut self, config: CacheConfig) -> Self {
        self.app = self.app.layer(CacheLayer::new(config));
        self
    }

    /// Get a reference to the server configuration.
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Get a clone of the underlying `axum::Router` with the built-in
    /// observability layers (request id, tracing) applied.
    ///
    /// Use this when serving manually; any additional layers (e.g. CORS) are
    /// applied outside the observability stack.
    pub fn app(&self) -> AxumRouter {
        with_builtin_layers(self.app.clone())
    }

    /// Bind to the configured address and serve until the process is killed.
    pub async fn serve(self) -> ServiceResult<()> {
        let listener = TcpListener::bind(self.config.addr)
            .await
            .map_err(|e| ServiceError::Internal(format!("bind {}: {}", self.config.addr, e)))?;
        self.serve_with_listener(listener).await
    }

    /// Serve on an already-bound listener (used by tests on port 0).
    pub async fn serve_with_listener(self, listener: TcpListener) -> ServiceResult<()> {
        axum::serve(listener, with_builtin_layers(self.app))
            .await
            .map_err(|e| ServiceError::Internal(format!("axum::serve: {}", e)))
    }

    /// Serve with graceful shutdown driven by the given future.
    pub async fn serve_with_shutdown<F>(self, shutdown: F) -> ServiceResult<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let listener = TcpListener::bind(self.config.addr)
            .await
            .map_err(|e| ServiceError::Internal(format!("bind {}: {}", self.config.addr, e)))?;
        axum::serve(listener, with_builtin_layers(self.app))
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|e| ServiceError::Internal(format!("axum::serve: {}", e)))
    }
}

/// Bind to port 0 and return the listener plus the bound address.
///
/// Useful for tests that want to discover a free port at runtime.
pub async fn bind_random_port(host: &str) -> ServiceResult<(TcpListener, SocketAddr)> {
    let listener = TcpListener::bind(format!("{host}:0"))
        .await
        .map_err(|e| ServiceError::Internal(format!("bind {host}:0: {}", e)))?;
    let addr = listener
        .local_addr()
        .map_err(|e| ServiceError::Internal(format!("local_addr: {e}")))?;
    Ok((listener, addr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_axum_server_new() {
        let server = AxumServer::new(ServiceRouter::new());
        assert_eq!(server.config().name, "sunbeam-server");
    }

    #[tokio::test]
    async fn test_bind_random_port_assigns_nonzero() {
        let (_listener, addr) = bind_random_port("127.0.0.1").await.unwrap();
        assert_ne!(addr.port(), 0);
    }

    #[tokio::test]
    async fn test_serve_and_shutdown() {
        let (listener, addr) = bind_random_port("127.0.0.1").await.unwrap();
        let server = AxumServer::new(ServiceRouter::new());

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            axum::serve(listener, server.app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });

        // Hit the bound port — empty router responds 404, which is a valid signal
        // that the listener is live.
        let url = format!("http://{addr}/__doesntexist");
        let resp = reqwest::get(&url).await.unwrap();
        assert!(resp.status().is_client_error() || resp.status().is_server_error());

        let _ = tx.send(());
        handle.await.unwrap();
    }
}
