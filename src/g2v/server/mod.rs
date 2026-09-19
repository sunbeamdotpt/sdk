//! Server implementations for Sunbeam services.
//!
//! This module provides server implementations using Axum and standalone
//! hyper servers.

#[cfg(feature = "g2v-server")]
pub mod axum;
#[cfg(feature = "g2v-server")]
/// Builder.
pub mod builder;
#[cfg(feature = "g2v-standalone")]
/// Standalone.
pub mod standalone;

use std::net::SocketAddr;

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Socket address to bind to.
    pub addr: SocketAddr,
    /// Server name.
    pub name: String,
    /// Maximum concurrent connections.
    pub max_connections: u32,
    /// TLS enabled.
    pub tls: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            addr: std::net::SocketAddr::from(([0, 0, 0, 0], 8080)),
            name: "sunbeam-server".to_string(),
            max_connections: 1000,
            tls: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.addr, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(config.name, "sunbeam-server");
        assert_eq!(config.max_connections, 1000);
        assert!(!config.tls);
    }
}
