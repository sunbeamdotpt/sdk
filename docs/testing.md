---
title: Testing with Testcontainers
description: Container builders for Sunbeam services and how to write Docker-backed integration tests.
tags:
  - testing
  - testcontainers
  - integration
category: guides
nav_order: 10
created_at: "2026-07-20"
related:
  - features.md
  - clients/openbao.md
---

# Integration Testing with Testcontainers

The `testing` feature provides opinionated
[testcontainers](https://docs.rs/testcontainers) builders for the services
deployed by Sunbeam, absorbed from the former `sunbeam-test` crate. Builders
default to dev-mode, single-node configurations with log-based readiness and
no published ports.

```toml
[dependencies]
sdk = { git = "https://github.com/sunbeamdotpt/sdk", default-features = false, features = ["testing"] }
```

## Quick start

```rust,no_run
use sdk::testing::OpenBao;

# #[tokio::main]
# async fn main() {
let container = OpenBao::new().publish_ports().start().await.unwrap();
let url = format!("{}/v1/sys/health", OpenBao::url(&container).await.unwrap());

let resp = reqwest::get(&url).await.unwrap();
assert!(resp.status().is_success());
# }
```

Call `.publish_ports()` (or `.publish_port()` for Postgres) when the test
host must reach the container — required on runtimes where bridge IPs are
unreachable from the host (e.g. lima on macOS). Each builder exposes a URL
helper that resolves host and dynamic port for the running container.

## Available builders

| Builder | Image default | Notes |
|---------|---------------|-------|
| `Postgres` | `postgres` | `ory/ory/ory` credentials |
| `OpenBao` | `openbao/openbao` | dev mode, known root token |
| `OpenSearch` | `opensearchproject/opensearch` | single-node, security disabled |
| `Tuwunel` | `ghcr.io/matrix-construct/tuwunel` | baked-in config, registration token |
| `LiveKit` | `livekit/livekit-server` | dev key pair (`devkey`/`devsecret`) |
| `Prometheus` | `prom/prometheus` | stock self-scraping config |
| `Loki` | `grafana/loki` | stock single-binary config |
| `Grafana` | `grafana/grafana` | default `admin`/`admin` |
| `Stalwart` | `stalwartlabs/mail-server` | admin password from container logs |
| `SearXng` | `searxng/searxng` | binds `0.0.0.0:8080` |
| `Headscale` | `headscale/headscale` | baked-in `config.yaml` |
| `OtelCollector` | `otel/opentelemetry-collector` | OTLP/HTTP receiver + debug exporter |
| `Kratos`, `Hydra`, `Keto` | `oryd/*` | ory suite, `serve public` |
| `OpenFga` | `openfga/openfga` | in-memory datastore |
| `SsoGateway` | `ghcr.io/sunbeamdotpt/sso-gateway` | full stack orchestrator |

## The SsoGateway orchestrator

`SsoGateway` boots the whole reference stack — Postgres, Hydra, Kratos, and a
permission backend (OpenFGA by default; switch with
`with_permissions_backend(PermissionBackend::Keto)`) — on a private Docker
network, then the gateway image itself. It exposes only `endpoint()` and
`shutdown()`:

```rust,no_run
use sdk::testing::SsoGateway;

# #[tokio::main]
# async fn main() {
let gateway = SsoGateway::new().start().await.unwrap();
let endpoint = gateway.endpoint(); // http://127.0.0.1:<random-port>
# }
```

The default image tag is `v2026.07.20`; override per-run with the
`SSO_GATEWAY_IMAGE_TAG` environment variable or `.with_image(name, tag)`.

## Writing container tests for SDK modules

Container-backed tests live next to the code in
`#[cfg(all(test, feature = "testing"))]` modules, so they compile only when
both `testing` and the module's own feature are enabled:

```rust,ignore
#[cfg(all(test, feature = "testing"))]
mod container_tests {
    #[tokio::test]
    async fn opensearch_doc_lifecycle() {
        let container = crate::testing::OpenSearch::default()
            .publish_ports()
            .start()
            .await
            .expect("opensearch should start");
        // …exercise the client against the container URL…
    }
}
```

Run them with `cargo nextest run --all-features --lib` (Docker required).
They are not run in CI.

## Docker host detection

`tests/support::init_docker_host()` points `DOCKER_HOST` at the active Docker
context when it is unset — call it before starting containers on macOS
(lima/colima/Docker Desktop).

## Heavy stacks and nextest

The sso-gateway e2e tests each boot a five-container stack. Because nextest
runs every test in its own process, an in-process mutex cannot serialize
them — `.config/nextest.toml` pins the `sso_gateway_client` binary to a
single-threaded test group instead. Reuse that pattern for any test that
boots more than a couple of containers.
