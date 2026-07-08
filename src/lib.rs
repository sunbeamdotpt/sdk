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
#[macro_use]
/// Error types and result aliases for the SDK.
pub mod error;

/// OAuth2 / SSO authentication commands.
pub mod auth;
/// Context-based configuration file I/O.
pub mod config;
/// Shared constants (paths, ports, timeouts).
pub mod constants;
/// Kubernetes client setup and manifest operations.
pub mod kube;
/// Structured logger with inherited fields.
pub mod logger;
/// Unified tracing-based logging subsystem.
pub mod logging;
/// Runtime manifest parameter discovery and override application.
pub mod manifest_params;
/// Kustomize build, apply, and namespace filtering.
pub mod manifests;
/// OpenBao (HashiCorp Vault fork) API client.
pub mod openbao;
/// Manifest-anchored profile override system.
pub mod profiles;
/// OpenBao secret reading and seeding.
pub mod secrets;
/// Vault transit keystore operations.
pub mod vault_keystore;
/// VPN integration — daemon control and environment detection.
pub mod vpn;
/// Workflow engine remote control client.
pub mod wfectl;

/// Kanban project management via ConnectRPC.
pub mod kanban;

// Private support modules used by public modules above.
mod exec;
mod registry;
mod tools;
