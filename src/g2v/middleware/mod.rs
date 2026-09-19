//! Middleware for Sunbeam services.

#[cfg(feature = "g2v-server")]
pub mod audit;
#[cfg(feature = "g2v-server")]
pub mod auth;
#[cfg(all(feature = "g2v-cache", feature = "g2v-server"))]
/// Response cache.
pub mod cache;
#[cfg(feature = "g2v-server")]
/// Instrumentation.
pub mod instrumentation;
#[cfg(feature = "g2v-server")]
/// Logging.
pub mod logging;
#[cfg(feature = "g2v-server")]
/// Rate limiting.
pub mod rate_limit;
#[cfg(feature = "g2v-server")]
/// Request id.
pub mod request_id;
#[cfg(feature = "g2v-server")]
/// Tracing.
pub mod tracing;

#[cfg(feature = "g2v-server")]
/// Re-export commonly used middleware
pub use instrumentation::InstrumentationLayer;

/// A boxed error type for middleware.
pub type MiddlewareError = Box<dyn std::error::Error + Send + Sync>;
