#![warn(missing_docs)]
#![deny(unused_mut)]
#![deny(clippy::missing_safety_doc)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![cfg_attr(not(test), deny(clippy::expect_used))]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
// just keeps syntax consistent
#![deny(clippy::needless_borrow)]
//! sdk — SDK for Sunbeam remote services, authentication, secrets, VPN, and
//! Kubernetes manifest tunables.
//!
//! Functional areas are gated behind cargo features; `default = ["full"]`
//! enables everything. Use `default-features = false` with an explicit
//! feature list to tree-shake.
#[macro_use]
/// Error types and result aliases for the SDK.
pub mod error;

/// sso-gateway IAM client (ConnectRPC).
#[cfg(feature = "auth")]
pub mod auth;
/// BuildKit container image build client.
#[cfg(feature = "build")]
pub mod build;
/// Context-based configuration file I/O.
pub mod config;
/// Shared constants (paths, ports, timeouts).
pub mod constants;
/// Kubernetes client setup and manifest operations.
#[cfg(feature = "kube")]
pub mod kube;
/// Structured logger with inherited fields.
pub mod logger;
/// Unified tracing-based logging subsystem.
pub mod logging;
/// Runtime manifest parameter discovery and override application.
#[cfg(feature = "kube")]
pub mod manifest_params;
/// Kustomize build, apply, and namespace filtering.
#[cfg(feature = "kube")]
pub mod manifests;
/// Matrix chat and collaboration API client.
#[cfg(feature = "matrix")]
pub mod matrix;
/// LiveKit real-time media API client.
#[cfg(feature = "media")]
pub mod media;
/// Monitoring clients — Prometheus, Loki, and Grafana.
#[cfg(feature = "monitoring")]
pub mod monitoring;
/// OpenBao (HashiCorp Vault fork) API client.
#[cfg(feature = "openbao")]
pub mod openbao;
/// Manifest-anchored profile override system.
#[cfg(feature = "kube")]
pub mod profiles;
/// OpenSearch search and analytics API client.
#[cfg(feature = "search")]
pub mod search;
/// OpenBao secret reading and seeding.
#[cfg(feature = "secrets")]
pub mod secrets;
/// Testcontainers builders for integration testing.
#[cfg(feature = "testing")]
pub mod testing;
/// Vault transit keystore operations.
#[cfg(feature = "vault-keystore")]
pub mod vault_keystore;
/// VPN integration — daemon control and environment detection.
#[cfg(feature = "vpn")]
pub mod vpn;
/// Workflow engine remote control client.
#[cfg(feature = "wfectl")]
pub mod wfectl;

/// Kanban project management via ConnectRPC.
#[cfg(feature = "kanban")]
pub mod kanban;

// Re-exports of public-API dependency crates. Consumers should name these
// types through the SDK instead of adding direct dependencies, so their
// versions always match the ones the SDK was compiled against (a second
// kube/reqwest in the graph causes type mismatches with `kube::get_client()`
// and the `From<reqwest::Error>` conversion). The kube crate is exported as
// `kube_rs` because the SDK has its own `kube` module.
#[cfg(feature = "kube")]
pub use ::kube as kube_rs;
#[cfg(feature = "kube")]
pub use k8s_openapi;
pub use reqwest;

// Private support modules used by public modules above.
#[cfg(feature = "kube")]
mod exec;
#[cfg(feature = "kube")]
mod registry;
#[cfg(feature = "kube")]
mod tools;
