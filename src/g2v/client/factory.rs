//! Client factory for creating service clients.

/// Factory for creating service clients.
#[derive(Debug, Clone)]
pub struct ClientFactory {
    /// Base URL for the service.
    base_url: String,
}

impl ClientFactory {
    /// Create a new client factory with the given base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Create a client factory from environment variables.
    /// Uses SERVICE_URL or defaults to localhost:8080.
    pub fn from_env() -> Self {
        let base_url =
            std::env::var("SERVICE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
        Self { base_url }
    }
}

impl Default for ClientFactory {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_factory_new() {
        let factory = ClientFactory::new("http://example.com");
        assert_eq!(factory.base_url(), "http://example.com");
    }

    #[test]
    fn test_client_factory_default() {
        let factory = ClientFactory::default();
        assert_eq!(factory.base_url(), "http://localhost:8080");
    }

    #[test]
    fn test_client_factory_from_env() {
        // SAFETY: This test mutates an environment variable that no other test
        // touches and restores it before returning.
        unsafe {
            let original = std::env::var("SERVICE_URL").ok();

            std::env::set_var("SERVICE_URL", "http://env.example.com");
            let factory = ClientFactory::from_env();
            assert_eq!(factory.base_url(), "http://env.example.com");

            std::env::remove_var("SERVICE_URL");
            let factory = ClientFactory::from_env();
            assert_eq!(factory.base_url(), "http://localhost:8080");

            match original {
                Some(v) => std::env::set_var("SERVICE_URL", v),
                None => std::env::remove_var("SERVICE_URL"),
            }
        }
    }
}
