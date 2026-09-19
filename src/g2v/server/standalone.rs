//! Standalone server implementation using hyper.

use crate::g2v::router::ServiceRouter;
use connectrpc::ConnectRpcService;

/// Standalone ConnectRPC server.
pub struct StandaloneServer {
    service: ConnectRpcService,
    config: super::ServerConfig,
}

impl StandaloneServer {
    /// Create a new standalone server with the given router.
    pub fn new(router: ServiceRouter) -> Self {
        let connect_router = router.into_inner();
        let service = ConnectRpcService::new(connect_router);

        Self {
            service,
            config: super::ServerConfig::default(),
        }
    }

    /// Create a new standalone server with custom configuration.
    pub fn with_config(router: ServiceRouter, config: super::ServerConfig) -> Self {
        let connect_router = router.into_inner();
        let service = ConnectRpcService::new(connect_router);

        Self { service, config }
    }

    /// Get a reference to the ConnectRpcService.
    pub fn service(&self) -> &ConnectRpcService {
        &self.service
    }

    /// Get a reference to the server configuration.
    pub fn config(&self) -> &super::ServerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standalone_server_new() {
        let service_router = ServiceRouter::new();
        let server = StandaloneServer::new(service_router);
        assert_eq!(server.config.name, "sunbeam-server");
    }

    #[test]
    fn test_standalone_server_with_config() {
        let config = crate::g2v::server::ServerConfig {
            addr: "127.0.0.1:9000".parse().unwrap(),
            name: "test-standalone".to_string(),
            max_connections: 500,
            tls: false,
        };
        let service_router = ServiceRouter::new();
        let server = StandaloneServer::with_config(service_router, config.clone());
        assert_eq!(server.config.name, config.name);
    }
}
