//! Shared integration-test support code.
//!
//! Orchestration of external services is delegated to the `sunbeam-test`
//! crate; this module re-exports the pieces used by SDK integration tests and
//! provides Docker environment detection.

pub mod docker;

pub use docker::init_docker_host;
pub use sunbeam_test::SsoGateway;
