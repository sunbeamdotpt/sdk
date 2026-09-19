//! Service router for ConnectRPC services.
//!
//! This module provides a `ServiceRouter` that wraps ConnectRPC's `Router` with
//! Sunbeam-specific functionality.

use connectrpc::Router as ConnectRouter;

/// A router for Sunbeam services.
///
/// This wraps ConnectRPC's Router to provide additional functionality
/// such as automatic service registration and middleware application.
pub struct ServiceRouter {
    inner: ConnectRouter,
    service_name: String,
}

impl ServiceRouter {
    /// Create a new ServiceRouter.
    pub fn new() -> Self {
        Self {
            inner: ConnectRouter::new(),
            service_name: "sunbeam-g2v".to_string(),
        }
    }

    /// Create a new ServiceRouter with a custom service name.
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            inner: ConnectRouter::new(),
            service_name: name.into(),
        }
    }

    /// Create a `ServiceRouter` from a pre-populated `ConnectRouter`.
    ///
    /// Use this when you have already registered Connect-RPC service handlers
    /// on a `ConnectRouter` (e.g. via a generated `*Ext::register` call) and
    /// want to hand the result to `ServerBuilder`.
    pub fn from_router(inner: ConnectRouter) -> Self {
        Self {
            inner,
            service_name: "sunbeam-g2v".to_string(),
        }
    }

    /// Get the service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Get a reference to the underlying ConnectRouter.
    pub fn inner(&self) -> &ConnectRouter {
        &self.inner
    }

    /// Consume and return the underlying ConnectRouter.
    pub fn into_inner(self) -> ConnectRouter {
        self.inner
    }
}

impl Default for ServiceRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_new() {
        let router = ServiceRouter::new();
        assert_eq!(router.service_name, "sunbeam-g2v");
    }

    #[test]
    fn test_router_with_name() {
        let router = ServiceRouter::with_name("my-service");
        assert_eq!(router.service_name, "my-service");
    }

    #[test]
    fn test_router_default() {
        let router = ServiceRouter::default();
        assert_eq!(router.service_name, "sunbeam-g2v");
    }

    #[test]
    fn test_router_inner() {
        let router = ServiceRouter::new();
        let _inner: &ConnectRouter = router.inner();
    }

    #[test]
    fn test_router_from_router_and_into_inner() {
        let router = ServiceRouter::from_router(ConnectRouter::new());
        assert_eq!(router.service_name(), "sunbeam-g2v");
        let _inner = router.into_inner();
    }
}
