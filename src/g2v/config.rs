//! Service configuration management.
//!
//! This module provides configuration types and loading utilities for Sunbeam services.

use figment::Figment;
use figment::providers::Env;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Configuration-specific error type.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// Failed to load configuration from the underlying provider.
    #[error("failed to load configuration: {0}")]
    Figment(Box<figment::Error>),

    /// Validation failed.
    #[error("configuration validation failed: {0}")]
    Validation(String),
}

impl From<figment::Error> for ConfigError {
    fn from(err: figment::Error) -> Self {
        ConfigError::Figment(Box::new(err))
    }
}

/// Main service configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceConfig {
    /// Service name.
    pub name: String,
    /// Environment (development, staging, production).
    pub environment: String,
    /// Host to bind to.
    pub host: String,
    /// Port to bind to.
    pub port: u16,
    /// Debug mode.
    pub debug: bool,
    /// Log level.
    pub log_level: String,
    /// Public base URL used for callbacks and token issuers.
    pub public_base_url: String,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            name: "sunbeam-g2v".to_string(),
            environment: "development".to_string(),
            host: "0.0.0.0".to_string(),
            port: 8080,
            debug: false,
            log_level: "info".to_string(),
            public_base_url: String::new(),
        }
    }
}

impl ServiceConfig {
    /// Load configuration from environment.
    pub fn load() -> Result<Self, ConfigError> {
        let config: Self = Figment::new().merge(Env::raw()).extract()?;
        Ok(config)
    }
}

/// Database configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    /// Database URL.
    pub url: String,
    /// Maximum connections.
    pub max_connections: u32,
    /// Connection timeout in seconds.
    pub connect_timeout: u64,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgres://localhost:5432/sunbeam".to_string(),
            max_connections: 10,
            connect_timeout: 30,
        }
    }
}

/// NATS configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NatsConfig {
    /// NATS server URL.
    pub url: String,
    /// JetStream enabled.
    pub jetstream: bool,
    /// Lease duration for leader election locks (seconds).
    pub lease_duration: u64,
    /// Optional explicit NATS token. Takes precedence over a token embedded
    /// in the URL userinfo.
    pub auth_token: Option<String>,
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            url: "nats://localhost:4222".to_string(),
            jetstream: true,
            lease_duration: 30,
            auth_token: None,
        }
    }
}

/// Authentication and authorization configuration.
///
/// Used by the multitenancy-aware auth middleware to validate API keys and
/// sessions, and by the authorization client to talk to the ReBAC backend.
#[cfg(feature = "g2v-server")]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    /// Authorization backend read URL (for permission checks and expansion).
    pub authorization_read_url: String,
    /// Authorization backend write URL (for relation tuple mutations).
    pub authorization_write_url: String,
    /// System tenant ULID, bootstrapped on first startup.
    pub system_tenant_ulid: String,
    /// Set `Secure` attribute on session cookies.
    pub cookie_secure: bool,
    /// `SameSite` attribute on session cookies (`strict`, `lax`, `none`).
    pub cookie_samesite: String,
    /// Session TTL in seconds.
    pub session_ttl_seconds: u64,
    /// Token introspection cache TTL in seconds.
    pub token_introspection_cache_ttl_seconds: u64,
    /// Secret used for signing state cookies and session tokens.
    pub state_cookie_secret: String,
}

#[cfg(feature = "g2v-server")]
impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            authorization_read_url: "http://localhost:4466".to_string(),
            authorization_write_url: "http://localhost:4467".to_string(),
            system_tenant_ulid: String::new(),
            cookie_secure: true,
            cookie_samesite: "strict".to_string(),
            session_ttl_seconds: 3600,
            token_introspection_cache_ttl_seconds: 60,
            state_cookie_secret: String::new(),
        }
    }
}

#[cfg(feature = "g2v-server")]
impl AuthConfig {
    /// Validate auth configuration.
    ///
    /// Checks:
    /// - `system_tenant_ulid` is a valid ULID if non-empty.
    /// - `state_cookie_secret` is at least 32 bytes if non-empty.
    /// - `state_cookie_secret` is not a known dev placeholder.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.system_tenant_ulid.is_empty()
            && ulid::Ulid::from_string(&self.system_tenant_ulid).is_err()
        {
            return Err(ConfigError::Validation(
                "system_tenant_ulid must be a valid ULID".into(),
            ));
        }

        if !self.state_cookie_secret.is_empty() {
            if self.state_cookie_secret.len() < 32 {
                return Err(ConfigError::Validation(
                    "state_cookie_secret must be at least 32 bytes".into(),
                ));
            }
            let lower = self.state_cookie_secret.to_ascii_lowercase();
            if lower.contains("change") || lower.contains("secret") || lower.contains("default") {
                return Err(ConfigError::Validation(
                    "state_cookie_secret appears to be a development placeholder; set a strong secret".into(),
                ));
            }
        }

        Ok(())
    }
}

/// Observability configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObservabilityConfig {
    /// Metrics enabled.
    pub metrics: bool,
    /// Tracing enabled.
    pub tracing: bool,
    /// Prometheus endpoint.
    pub prometheus_endpoint: String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            metrics: true,
            tracing: true,
            prometheus_endpoint: "/metrics".to_string(),
        }
    }
}

/// Rate limiting configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LimitsConfig {
    /// Max requests per second.
    pub max_requests_per_sec: u64,
    /// Burst size.
    pub burst_size: u64,
    /// Public rate limit: max requests per window.
    pub public_rate_limit_requests: u32,
    /// Public rate limit: window duration in seconds.
    pub public_rate_limit_window_seconds: u64,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_requests_per_sec: 100,
            burst_size: 50,
            public_rate_limit_requests: 100,
            public_rate_limit_window_seconds: 60,
        }
    }
}

/// Leader election configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ElectionConfig {
    /// Election enabled.
    pub enabled: bool,
    /// Election strategy (vault, nats, memory).
    pub strategy: String,
    /// Lease duration in seconds.
    pub lease_duration: u64,
}

impl Default for ElectionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            strategy: "memory".to_string(),
            lease_duration: 30,
        }
    }
}

/// Vault configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VaultConfig {
    /// Vault server URL.
    pub url: String,
    /// Vault token.
    pub token: String,
    /// KV secret path.
    pub kv_path: String,
    /// Lease duration in seconds.
    pub lease_duration: u64,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:8200".to_string(),
            token: "root".to_string(),
            kv_path: "secret/sunbeam".to_string(),
            lease_duration: 30,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_config_default() {
        let config = ServiceConfig::default();
        assert_eq!(config.name, "sunbeam-g2v");
        assert_eq!(config.environment, "development");
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert!(!config.debug);
        assert_eq!(config.log_level, "info");
        assert_eq!(config.public_base_url, "");
    }

    #[test]
    fn test_database_config_default() {
        let config = DatabaseConfig::default();
        assert_eq!(config.url, "postgres://localhost:5432/sunbeam");
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.connect_timeout, 30);
    }

    #[test]
    fn test_nats_config_default() {
        let config = NatsConfig::default();
        assert_eq!(config.url, "nats://localhost:4222");
        assert!(config.jetstream);
        assert!(config.auth_token.is_none());
    }

    #[cfg(feature = "g2v-server")]
    #[test]
    fn test_auth_config_default() {
        let config = AuthConfig::default();
        assert_eq!(config.authorization_read_url, "http://localhost:4466");
        assert_eq!(config.authorization_write_url, "http://localhost:4467");
        assert!(config.system_tenant_ulid.is_empty());
        assert!(config.cookie_secure);
        assert_eq!(config.cookie_samesite, "strict");
        assert_eq!(config.session_ttl_seconds, 3600);
        assert_eq!(config.token_introspection_cache_ttl_seconds, 60);
    }

    #[cfg(feature = "g2v-server")]
    #[test]
    fn test_auth_config_validate_accepts_empty() {
        let config = AuthConfig::default();
        assert!(config.validate().is_ok());
    }

    #[cfg(feature = "g2v-server")]
    #[test]
    fn test_auth_config_validate_rejects_invalid_ulid() {
        let config = AuthConfig {
            system_tenant_ulid: "not-a-ulid".into(),
            ..AuthConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[cfg(feature = "g2v-server")]
    #[test]
    fn test_auth_config_validate_rejects_short_secret() {
        let config = AuthConfig {
            state_cookie_secret: "short".into(),
            ..AuthConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[cfg(feature = "g2v-server")]
    #[test]
    fn test_auth_config_validate_rejects_placeholder_secret() {
        let config = AuthConfig {
            state_cookie_secret: "change-me-in-production-cookie-secret".into(),
            ..AuthConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_observability_config_default() {
        let config = ObservabilityConfig::default();
        assert!(config.metrics);
        assert!(config.tracing);
        assert_eq!(config.prometheus_endpoint, "/metrics");
    }

    #[test]
    fn test_limits_config_default() {
        let config = LimitsConfig::default();
        assert_eq!(config.max_requests_per_sec, 100);
        assert_eq!(config.burst_size, 50);
        assert_eq!(config.public_rate_limit_requests, 100);
        assert_eq!(config.public_rate_limit_window_seconds, 60);
    }

    #[test]
    fn test_election_config_default() {
        let config = ElectionConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.strategy, "memory");
        assert_eq!(config.lease_duration, 30);
    }

    #[test]
    fn test_vault_config_default() {
        let config = VaultConfig::default();
        assert_eq!(config.url, "http://localhost:8200");
        assert_eq!(config.token, "root");
        assert_eq!(config.kv_path, "secret/sunbeam");
    }
}
