//! Testcontainers builders — throwaway containers for Sunbeam service integration tests.
//!
//! Each module provides a small, opinionated builder that spins up a throwaway
//! container for integration tests:
//!
//! * [`Kratos`](kratos::Kratos) – Ory identity & user management
//! * [`Hydra`](hydra::Hydra) – Ory OAuth2 / OIDC provider
//! * [`Keto`](keto::Keto) – Ory authorization / permission engine
//! * [`OpenFga`](openfga::OpenFga) – OpenFGA authorization / permission engine
//! * [`Postgres`](postgres::Postgres) – PostgreSQL metadata store
//! * [`SsoGateway`](sso_gateway::SsoGateway) – full sso-gateway stack (pre-built image + deps)
//! * [`Nats`](nats::Nats) – messaging and JetStream
//! * [`OpenBao`](openbao::OpenBao) – secrets management
//! * [`OpenSearch`](opensearch::OpenSearch) – search & analytics
//! * [`Stalwart`](stalwart::Stalwart) – mail server (JMAP/IMAP/SMTP)
//! * [`SearXng`](searxng::SearXng) – privacy metasearch
//! * [`Headscale`](headscale::Headscale) – self-hosted Tailscale control server
//! * [`Tuwunel`](tuwunel::Tuwunel) – Matrix homeserver
//! * [`OtelCollector`](otelcol::OtelCollector) – OpenTelemetry collector
//! * [`Kanban`](kanban::Kanban) – full kanban server stack (pre-built image + deps;
//!   requires the `auth` feature)
//!
//! The image tags match the versions pinned by the Sunbeam deployment.
//!
//! | Builder | Image default | Notes |
//! |---------|---------------|-------|
//! | [`Kratos`] | `oryd/kratos` | Identity server, runs `serve public` |
//! | [`Hydra`] | `oryd/hydra` | OAuth2/OIDC server, runs `serve public` |
//! | [`Keto`] | `oryd/keto` | Permission server, runs `serve` |
//! | [`OpenFga`] | `openfga/openfga` | ReBAC permission server, in-memory datastore |
//! | [`OpenBao`] | `openbao/openbao` | Dev mode (auto-unsealed) with a known root token |
//! | [`OpenSearch`] | `opensearchproject/opensearch` | Single-node cluster with security disabled |
//! | [`OtelCollector`] | `otel/opentelemetry-collector` | Derived image with a baked-in config; OTLP/HTTP receiver + debug exporter logging received spans |
//! | [`Stalwart`] | `stalwartlabs/mail-server` | Bootstrap mode; admin password is read from container logs |
//! | [`SearXng`] | `searxng/searxng` | Binds on `0.0.0.0:8080` so it is reachable from the bridge network |
//! | [`Headscale`] | `headscale/headscale` | Derived image with a baked-in `config.yaml` |
//! | [`Tuwunel`] | `ghcr.io/matrix-construct/tuwunel` | Derived image with a baked-in `tuwunel.toml` |
//! | [`Nats`] | `nats` | Messaging / JetStream; optional published port |
//! | [`Postgres`] | `postgres` | `ory/ory/ory` credentials; optional published port |
//! | [`SsoGateway`] | `ghcr.io/sunbeamdotpt/sso-gateway` | Full stack (Postgres + Hydra + Kratos + a permission backend + gateway image) on a private network; exposes a single endpoint. Permission backend is OpenFGA by default; switch with `with_permissions_backend` |
//! | [`Kanban`] | `ghcr.io/sunbeamdotpt/kanban` | Full stack (Postgres + NATS + OpenSearch + MinIO + an [`SsoGateway`] stack + kanban image) on a private network; provisions the `kanban-test` tenant and service credentials via IAM. Requires the `auth` feature |
//!
//! The builders default to an in-memory / single-node / dev-mode configuration and a
//! log-based readiness check. When you need to reach a container from the test host,
//! call `.publish_ports()` (or `.publish_port()` for Postgres) before `.start()` and
//! then use the module's URL helper (for example [`Kratos::public_url`]):
//!
//! ```rust,no_run
//! use sdk::testing::OpenBao;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let container = OpenBao::new().publish_ports().start().await.unwrap();
//! let url = format!("{}/v1/sys/health", OpenBao::url(&container).await.unwrap());
//!
//! let resp = reqwest::get(&url).await.unwrap();
//! assert!(resp.status().is_success());
//! # }
//! ```
//!
//! Most builders expose a fluent API for tags, config overrides, and credentials:
//!
//! ```rust,no_run
//! # #[tokio::main]
//! # async fn main() {
//! let container = sdk::testing::OpenBao::new()
//!     .with_tag("2.1.0")
//!     .with_root_token("my-token")
//!     .publish_ports()
//!     .start()
//!     .await
//!     .unwrap();
//! # }
//! ```
//!
//! For sso-gateway, use the orchestrator to start the whole stack from a pre-built
//! image; the permission backend defaults to OpenFGA, which matches the published
//! gateway image (built with default Cargo features: `openfga`, not `keto`). Switch
//! backends with [`SsoGateway::with_permissions_backend`]:
//!
//! ```rust,no_run
//! use sdk::testing::{PermissionBackend, SsoGateway};
//!
//! # #[tokio::main]
//! # async fn main() {
//! // OpenFGA (default) — works with the published `latest` image.
//! let gateway = SsoGateway::new().start().await.unwrap();
//! let endpoint = gateway.endpoint(); // http://127.0.0.1:<random-port>
//!
//! // Keto — requires a gateway image built with the `keto` Cargo feature.
//! let gateway = SsoGateway::new()
//!     .with_image("my-registry/sso-gateway", "keto")
//!     .with_permissions_backend(PermissionBackend::Keto)
//!     .start()
//!     .await
//!     .unwrap();
//! # }
//! ```
//!
//! The tests work with any Docker-compatible runtime. They connect to containers
//! using published ports and dynamic host ports rather than bridge-network IPs, so
//! setups where bridge IPs are unreachable from the host (e.g. lima on macOS) work
//! as long as `DOCKER_HOST` points at the runtime's socket.

/// Grafana — dashboards container builder (default admin credentials).
pub mod grafana;
/// Headscale — self-hosted Tailscale control server container builder.
pub mod headscale;
/// Ory Hydra — OAuth2 / OIDC provider container builder.
pub mod hydra;
/// Kanban — full kanban server stack orchestrator (requires the `auth`
/// feature for IAM provisioning).
#[cfg(feature = "auth")]
pub mod kanban;
/// Ory Keto — authorization / permission engine container builder.
pub mod keto;
/// Ory Kratos — identity & user management container builder.
pub mod kratos;
/// LiveKit — real-time media server container builder (dev keys).
pub mod livekit;
/// Loki — log aggregation container builder (stock single-binary config).
pub mod loki;
/// NATS — messaging and JetStream container builder.
pub mod nats;
/// OpenBao — dev-mode secrets container builder.
pub mod openbao;
/// OpenFGA — ReBAC permission server container builder.
pub mod openfga;
/// OpenSearch — single-node search & analytics container builder.
pub mod opensearch;
/// OpenTelemetry collector — OTLP/HTTP receiver container builder.
pub mod otelcol;
/// PostgreSQL — metadata store container builder.
pub mod postgres;
/// Prometheus — metrics container builder (stock config).
pub mod prometheus;
/// SearXNG — privacy metasearch container builder.
pub mod searxng;
/// sso-gateway — full-stack orchestrator (pre-built image + deps).
pub mod sso_gateway;
/// Stalwart — mail server (JMAP/IMAP/SMTP) container builder.
pub mod stalwart;
/// Tuwunel — Matrix homeserver container builder.
pub mod tuwunel;

pub(crate) mod util;

pub use grafana::Grafana;
pub use headscale::Headscale;
pub use hydra::Hydra;
#[cfg(feature = "auth")]
pub use kanban::{Kanban, KanbanHandle};
pub use keto::Keto;
pub use kratos::Kratos;
pub use livekit::LiveKit;
pub use loki::Loki;
pub use nats::Nats;
pub use openbao::OpenBao;
pub use openfga::OpenFga;
pub use opensearch::OpenSearch;
pub use otelcol::OtelCollector;
pub use postgres::Postgres;
pub use prometheus::Prometheus;
pub use searxng::SearXng;
pub use sso_gateway::{PermissionBackend, SsoGateway, SsoGatewayHandle};
pub use stalwart::Stalwart;
pub use tuwunel::Tuwunel;
pub use util::{container_bridge_ip, container_host_url};

/// Pin `aws-lc-rs` as the process-wide rustls crypto provider (test support).
///
/// Dev builds can unify *both* rustls provider features — ring enters the
/// graph via wfe (kube 3.1, sqlx) while every edge this crate controls
/// selects aws-lc-rs — and rustls refuses to auto-select a provider when
/// both are present. Installing the default explicitly keeps TLS connections
/// (remote Docker daemons, TLS-secured services) deterministic. Best-effort:
/// if a provider is already installed, that one stays.
#[cfg(test)]
pub fn install_default_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .is_err()
        {
            eprintln!("rustls crypto provider already installed; keeping existing default");
        }
    });
}

/// Prepare the container-test environment before any container starts.
///
/// Ensures `DOCKER_HOST` points at the active Docker context when it is not
/// already set (testcontainers reads `DOCKER_HOST` directly; the Docker CLI's
/// context-aware endpoint is not picked up on its own). For TLS-secured
/// remote daemons the context's CA/cert/key material is mirrored into
/// `DOCKER_CERT_PATH` / `DOCKER_TLS_VERIFY` and the endpoint rewritten to
/// `https://`. Also pins the rustls crypto provider (see
/// [`install_default_crypto_provider`]).
#[cfg(test)]
pub fn init_docker_host() {
    install_default_crypto_provider();

    if std::env::var("DOCKER_HOST").is_ok() {
        return;
    }

    let output = std::process::Command::new("docker")
        .args([
            "context",
            "inspect",
            "-f",
            "{{.Endpoints.docker.Host}}\n{{.Storage.TLSPath}}",
        ])
        .output();

    let (mut host, tls_dir) = match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
            // Missing lines mean an unusual context shape; the emptiness
            // handling below falls back to the default socket.
            let host = lines.next().unwrap_or("").to_string();
            let tls = lines.next().unwrap_or("").to_string();
            (host, tls)
        }
        _ => (String::new(), String::new()),
    };

    if host.is_empty() {
        // Fall back to the default unix socket. If Docker isn't there,
        // testcontainers fails with a clear connection error.
        // SAFETY: single-threaded test-process init; env vars are read later
        // by testcontainers when the first container starts.
        unsafe { std::env::set_var("DOCKER_HOST", "unix:///var/run/docker.sock") };
        return;
    }

    let ca = std::path::Path::new(&tls_dir).join("docker").join("ca.pem");
    if host.starts_with("tcp://") && ca.exists() {
        if std::env::var_os("DOCKER_CERT_PATH").is_none() {
            // SAFETY: same single-threaded test-init window as above.
            unsafe {
                std::env::set_var("DOCKER_CERT_PATH", ca.parent().unwrap());
                std::env::set_var("DOCKER_TLS_VERIFY", "1");
            }
        }
        host = host.replacen("tcp://", "https://", 1);
    }

    // SAFETY: same single-threaded test-init window as above.
    unsafe { std::env::set_var("DOCKER_HOST", host) };
}
