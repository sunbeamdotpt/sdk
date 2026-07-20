---
title: Sunbeam SDK
description: Rust SDK for Sunbeam workspace management — service clients, Kubernetes manifests, VPN, OpenBao secrets, and WFE workflow orchestration.
category: reference
nav_order: 0
related:
  - getting-started.md
  - features.md
  - testing.md
---

# Sunbeam SDK

The **Sunbeam SDK** is a Rust library for building Sunbeam-compatible tooling:
remote service clients (sso-gateway, Kanban, WFE, OpenSearch, Matrix, LiveKit,
monitoring), Kubernetes manifest management, VPN integration, OpenBao secrets,
and testcontainer builders for integration testing.

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org/)

## Using the SDK

```toml
[dependencies]
sdk = { git = "https://github.com/sunbeamdotpt/sdk" }
```

Every functional area is a cargo feature; `default = ["full"]` enables
everything. To tree-shake, opt in explicitly:

```toml
sdk = { git = "https://github.com/sunbeamdotpt/sdk", default-features = false, features = ["search", "media"] }
```

## Quick start

Service clients are built on the
[`sunbeam-g2v`](https://crates.io/crates/sunbeam-g2v) client stack. Build one
shared `Client` with your auth and hand it to each service client:

```rust,no_run
use sdk::search::OpenSearchClient;
use sunbeam_g2v::client::{BearerToken, ClientBuilder};

# async fn example() -> sdk::error::Result<()> {
let g2v = ClientBuilder::new("https://search.example.com/")
    .auth(BearerToken::new("my-token"))
    .build()
    .expect("valid base URL");
let search = OpenSearchClient::new(&g2v);

let health = search.cluster_health().await?;
println!("cluster: {}", health.status);
# Ok(())
# }
```

> **Note:** when the base URL contains a path it must end with a trailing
> slash (e.g. `https://host/_matrix/`), otherwise the last segment is dropped
> when request paths are joined.

## Feature flags

| Feature | What you get |
|---------|--------------|
| `auth` | sso-gateway IAM client (ConnectRPC) |
| `kanban` | Kanban project management client |
| `wfectl` | WFE workflow engine gRPC client |
| `search` | OpenSearch API client |
| `matrix` | Matrix Client-Server API client |
| `media` | LiveKit Twirp client + JWT access tokens |
| `monitoring` | Prometheus, Loki, and Grafana clients |
| `build` | BuildKit image builds via `buildctl` |
| `kube` | Kubernetes client + kustomize manifests |
| `openbao` | OpenBao/Vault API client |
| `secrets` | OpenBao init/unseal/seed (enables `kube` + `openbao`) |
| `vault-keystore` | Vault transit keystore crypto |
| `vpn` | VPN daemon control + kube proxy hook |
| `testing` | Testcontainers builders for Sunbeam services |

See [Features](docs/features.md) for the full reference.

## Documentation

- [Getting started](docs/getting-started.md)
- [Feature flags](docs/features.md)
- Clients: [auth](docs/clients/auth.md) · [search](docs/clients/search.md) ·
  [matrix](docs/clients/matrix.md) · [media](docs/clients/media.md) ·
  [monitoring](docs/clients/monitoring.md) · [build](docs/clients/build.md) ·
  [kanban](docs/clients/kanban.md) · [wfectl](docs/clients/wfectl.md) ·
  [openbao](docs/clients/openbao.md)
- [Integration testing with testcontainers](docs/testing.md)
- [Configuration](docs/configuration.md) · [Error handling](docs/error-handling.md)

## Building and testing

```bash
cargo build --release
cargo nextest run --lib                      # unit tests, no Docker needed
cargo nextest run --all-features --lib       # + container tests (needs Docker)
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

## License

MIT — see [LICENSE](LICENSE.md).
