//! Service trait and base implementation.
//!
//! This module defines the `SunbeamService` trait which all Sunbeam services implement.

use crate::g2v::error::ServiceResult;
use std::future::Future;
use std::sync::Arc;

/// Trait that all Sunbeam services implement.
///
/// This trait provides a common interface for service implementations,
/// including lifecycle hooks and request handling.
pub trait SunbeamService: Send + Sync + 'static {
    /// Returns the service name.
    fn name(&self) -> &str;

    /// Returns the service version.
    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }
}

/// Extension trait for SunbeamService with convenience methods.
pub trait ServiceExt: SunbeamService {
    /// Wrap the service in an Arc for sharing.
    fn shared(self) -> Arc<Self>
    where
        Self: Sized,
    {
        Arc::new(self)
    }

    /// Convert to a boxed trait object.
    fn boxed(self) -> Box<dyn SunbeamService>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }

    /// Called when the service starts.
    fn on_start(&self) -> impl Future<Output = ServiceResult<()>> + Send {
        async { Ok(()) }
    }

    /// Called when the service stops.
    fn on_stop(&self) -> impl Future<Output = ServiceResult<()>> + Send {
        async { Ok(()) }
    }

    /// Check if the service is healthy.
    fn health_check(&self) -> impl Future<Output = ServiceResult<()>> + Send {
        async { Ok(()) }
    }
}

impl<T: SunbeamService> ServiceExt for T {}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestService;

    impl SunbeamService for TestService {
        fn name(&self) -> &str {
            "test-service"
        }
    }

    #[test]
    fn test_service_name() {
        let service = TestService;
        assert_eq!(service.name(), "test-service");
    }

    #[test]
    fn test_service_version() {
        let service = TestService;
        assert!(!service.version().is_empty());
    }

    #[test]
    fn test_service_shared() {
        let service = TestService;
        let shared = service.shared();
        assert_eq!(shared.name(), "test-service");
    }

    #[test]
    fn test_service_boxed() {
        let service = TestService;
        let boxed: Box<dyn SunbeamService> = service.boxed();
        assert_eq!(boxed.name(), "test-service");
    }

    #[tokio::test]
    async fn test_service_lifecycle() {
        struct LifecycleService;

        impl SunbeamService for LifecycleService {
            fn name(&self) -> &str {
                "lifecycle-service"
            }
        }

        let service = LifecycleService;
        service.on_start().await.unwrap();
        service.on_stop().await.unwrap();
        service.health_check().await.unwrap();
    }
}
