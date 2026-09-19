//! Server builder for easy server configuration.
//!
//! # Health checks
//!
//! Use [`ServerBuilder::with_health`](crate::g2v::server::builder::ServerBuilder::with_health) to attach a [`HealthRouter`](crate::g2v::health::HealthRouter) that exposes
//! `GET /health/live` and `GET /health/ready` alongside your service routes:
//!
//! ```rust,no_run
//! # use std::sync::Arc;
//! # use crate::g2v::health::{HealthRouter, HttpHealthCheck};
//! # use crate::g2v::server::builder::ServerBuilder;
//! # use crate::g2v::router::ServiceRouter;
//! let server = ServerBuilder::new()
//!     .with_router(ServiceRouter::new())
//!     .with_health(
//!         HealthRouter::new()
//!             .with_check(Arc::new(HttpHealthCheck::with_url("upstream", "http://localhost:9999")))
//!     )
//!     .build_axum()
//!     .unwrap();
//! ```

#[cfg(feature = "g2v-server")]
use std::sync::Arc;

use axum::Router as AxumRouter;

use super::{ServerConfig, axum::AxumServer};
use crate::g2v::error::{ServiceError, ServiceResult};
use crate::g2v::health::HealthRouter;
#[cfg(feature = "g2v-server")]
use crate::g2v::metrics::ServiceMetrics;
#[cfg(feature = "g2v-cache")]
use crate::g2v::middleware::cache::CacheConfig;
use crate::g2v::router::ServiceRouter;

/// Builder for creating configured servers.
pub struct ServerBuilder {
    router: Option<ServiceRouter>,
    config: ServerConfig,
    extra_routes: Option<AxumRouter>,
    #[cfg(feature = "g2v-server")]
    metrics: Option<Arc<ServiceMetrics>>,
    #[cfg(feature = "g2v-cache")]
    cache: Option<CacheConfig>,
}

impl ServerBuilder {
    /// Create a new server builder with default configuration.
    pub fn new() -> Self {
        Self {
            router: None,
            config: ServerConfig::default(),
            extra_routes: None,
            #[cfg(feature = "g2v-server")]
            metrics: None,
            #[cfg(feature = "g2v-cache")]
            cache: None,
        }
    }

    /// Set the ConnectRPC service router.
    pub fn with_router(mut self, router: ServiceRouter) -> Self {
        self.router = Some(router);
        self
    }

    /// Override the server configuration.
    pub fn with_config(mut self, config: ServerConfig) -> Self {
        self.config = config;
        self
    }

    /// Merge extra axum routes (e.g. `/health`, `/metrics`) alongside the RPC handlers.
    pub fn with_routes(mut self, routes: AxumRouter) -> Self {
        self.extra_routes = match self.extra_routes.take() {
            Some(existing) => Some(existing.merge(routes)),
            None => Some(routes),
        };
        self
    }

    /// Attach a [`HealthRouter`] that adds `GET /health/live` and
    /// `GET /health/ready` to the server.
    ///
    /// Equivalent to `with_routes(health_router.into_axum_router())`.
    pub fn with_health(self, health_router: HealthRouter) -> Self {
        self.with_routes(health_router.into_axum_router())
    }

    /// Attach a `ServiceMetrics` instance. Currently retained for downstream wiring;
    /// the Tower instrumentation layer is applied separately by the caller.
    #[cfg(feature = "g2v-server")]
    pub fn with_metrics(mut self, metrics: Arc<ServiceMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Enable the response cache with the given configuration.
    ///
    /// The cache layer is applied just above the ConnectRPC router, before any
    /// additional layers added by the caller via [`AxumServer::app`].
    #[cfg(feature = "g2v-cache")]
    pub fn with_cache(mut self, config: CacheConfig) -> Self {
        self.cache = Some(config);
        self
    }

    /// Build an `AxumServer` without serving.
    pub fn build_axum(self) -> ServiceResult<AxumServer> {
        let router = self
            .router
            .ok_or_else(|| ServiceError::Configuration("Router is required".to_string()))?;

        let mut server = AxumServer::with_config(router, self.config);
        if let Some(extra) = self.extra_routes {
            server = server.merge(extra);
        }

        #[cfg(feature = "g2v-cache")]
        if let Some(cache) = self.cache {
            server = server.with_cache(cache);
        }

        Ok(server)
    }

    /// Build and serve until the process is killed.
    pub async fn serve(self) -> ServiceResult<()> {
        self.build_axum()?.serve().await
    }

    /// Build and serve with graceful shutdown driven by the given future.
    pub async fn serve_with_shutdown<F>(self, shutdown: F) -> ServiceResult<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.build_axum()?.serve_with_shutdown(shutdown).await
    }
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_new() {
        let builder = ServerBuilder::new();
        assert!(builder.router.is_none());
        assert_eq!(builder.config.name, "sunbeam-server");
    }

    #[test]
    fn test_builder_with_router() {
        let builder = ServerBuilder::new().with_router(ServiceRouter::new());
        assert!(builder.router.is_some());
    }

    #[test]
    fn test_builder_build_axum() {
        let server = ServerBuilder::new()
            .with_router(ServiceRouter::new())
            .build_axum()
            .unwrap();
        assert_eq!(server.config().name, "sunbeam-server");
    }

    #[test]
    fn test_builder_build_axum_no_router() {
        let result = ServerBuilder::new().build_axum();
        assert!(matches!(result, Err(ServiceError::Configuration(_))));
    }

    #[cfg(feature = "g2v-server")]
    #[test]
    fn test_builder_with_metrics() {
        let metrics = Arc::new(ServiceMetrics::new("test-service").unwrap());
        let builder = ServerBuilder::new()
            .with_router(ServiceRouter::new())
            .with_metrics(Arc::clone(&metrics));

        assert!(builder.metrics.is_some());
    }

    #[tokio::test]
    async fn test_builder_serve_with_shutdown() {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();

        let config = ServerConfig {
            addr: "127.0.0.1:0".parse().unwrap(),
            ..ServerConfig::default()
        };

        let server = ServerBuilder::new()
            .with_router(ServiceRouter::new())
            .with_config(config)
            .build_axum()
            .unwrap();

        // Trigger shutdown immediately so the test doesn't hang on bind-then-wait.
        let _ = tx.send(());
        server
            .serve_with_shutdown(async move {
                let _ = rx.await;
            })
            .await
            .unwrap();
    }

    #[test]
    fn test_builder_default_and_extra_routes_merge() {
        use axum::{Router, routing::get};

        let builder = ServerBuilder::default()
            .with_router(ServiceRouter::with_name("routes-service"))
            .with_routes(Router::new().route("/one", get(|| async { "one" })))
            .with_routes(Router::new().route("/two", get(|| async { "two" })))
            .with_health(crate::g2v::health::HealthRouter::new());
        assert!(builder.extra_routes.is_some());

        let server = builder.build_axum().unwrap();
        assert_eq!(server.config().name, "sunbeam-server");
    }

    #[cfg(feature = "g2v-cache")]
    #[test]
    fn test_builder_with_cache_builds() {
        use std::num::NonZeroUsize;

        let server = ServerBuilder::new()
            .with_router(ServiceRouter::new())
            .with_cache(CacheConfig {
                capacity: NonZeroUsize::new(4).unwrap(),
                ttl: None,
                max_body_size: 128,
            })
            .build_axum()
            .unwrap();
        assert_eq!(server.config().name, "sunbeam-server");
    }

    #[tokio::test]
    async fn test_builder_serve_reports_bind_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let result = ServerBuilder::new()
            .with_router(ServiceRouter::new())
            .with_config(ServerConfig {
                addr,
                ..ServerConfig::default()
            })
            .serve()
            .await;
        assert!(matches!(result, Err(ServiceError::Internal(_))));
    }
}
