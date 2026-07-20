---
title: Getting Started
description: Install the Sunbeam SDK, pick your features, and make your first API calls.
tags:
  - onboarding
  - quickstart
category: guides
nav_order: 1
created_at: "2026-07-20"
related:
  - features.md
  - configuration.md
  - error-handling.md
---

# Getting Started

## Install

```toml
[dependencies]
sdk = { git = "https://github.com/sunbeamdotpt/sdk" }
```

The default feature set (`full`) includes every module. For smaller builds,
disable defaults and opt in:

```toml
[dependencies]
sdk = { git = "https://github.com/sunbeamdotpt/sdk", default-features = false, features = ["search"] }
```

## Pick a client

| I want to… | Feature | Client |
|---|---|---|
| Call the sso-gateway IAM API | `auth` | `sdk::auth::AuthClient` |
| Search / analytics | `search` | `sdk::search::OpenSearchClient` |
| Chat (Matrix) | `matrix` | `sdk::matrix::MatrixClient` |
| Real-time media rooms | `media` | `sdk::media::LiveKitClient` |
| Metrics, logs, dashboards | `monitoring` | `sdk::monitoring::{PrometheusClient, LokiClient, GrafanaClient}` |
| Manage Kanban boards | `kanban` | `sdk::kanban::*` |
| Run workflows remotely | `wfectl` | `sdk::wfectl::*` |
| Build container images | `build` | `sdk::build::*` |
| Work with a cluster | `kube` | `sdk::kube`, `sdk::manifests` |
| Read/write secrets | `openbao`, `secrets` | `sdk::openbao::BaoClient` |

## Authentication

REST clients (`search`, `matrix`, `media`, `monitoring`) are built on the
`sunbeam-g2v` client stack. Create one `Client` per base URL and share it:

```rust,no_run
use sunbeam_g2v::client::{BearerToken, ClientBuilder};

let g2v = ClientBuilder::new("https://search.example.com/")
    .auth(BearerToken::new("access-token"))
    .build()
    .expect("valid base URL");
```

Every `TokenProvider` works — `BearerToken` for static tokens,
`OAuth2ClientCredentials` for machine-to-machine flows, or your own
implementation for rotating tokens. Retry policy, timeouts, circuit breakers,
and default headers are configured on the same builder.

Each client also has a `connect(domain)` convenience constructor that builds
an **unauthenticated** client for the standard hostname
(`https://search.{domain}`, `https://messages.{domain}/_matrix/`, …).

## Trailing-slash rule

Request paths are resolved with RFC 3986 URL joining. If the base URL has a
path component, it must end with `/`:

```rust
// WRONG — joins resolve to https://host/client/v3/... (drops "_matrix")
ClientBuilder::new("https://host/_matrix")

// RIGHT — joins resolve to https://host/_matrix/client/v3/...
ClientBuilder::new("https://host/_matrix/")
```

Root URLs (`https://host`, `https://host:9200`) are normalized automatically
and need no slash.

## Errors

Every fallible call returns `sdk::error::Result<T>` (`SunbeamError`). Add
context as errors bubble up with `ResultExt`:

```rust,no_run
use sdk::error::ResultExt;

# async fn example(client: &sdk::search::OpenSearchClient) -> sdk::error::Result<()> {
let health = client
    .cluster_health()
    .await
    .ctx("checking cluster health")?;
# Ok(())
# }
```

See [Error handling](error-handling.md) for variants and exit codes.

## Next steps

- [Feature flags reference](features.md)
- [Integration testing with testcontainers](testing.md)
- [Configuration](configuration.md)
