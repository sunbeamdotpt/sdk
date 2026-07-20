//! Shared integration-test support code.
//!
//! Orchestration of external services is delegated to the SDK's `testing`
//! module; this module re-exports the pieces used by SDK integration tests and
//! provides Docker environment detection.

pub mod docker;

pub use docker::init_docker_host;
#[cfg(feature = "testing")]
pub use sdk::testing::SsoGateway;
