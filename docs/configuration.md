---
title: Configuration
description: Context-based configuration in ~/.sunbeam/config.json and the SDK's global state.
tags:
  - configuration
  - contexts
category: reference
nav_order: 11
created_at: "2026-07-20"
related:
  - getting-started.md
  - error-handling.md
---

# Configuration

User configuration lives in `~/.sunbeam/config.json` (`SunbeamConfig`). It
holds multiple named `Context`s plus the name of the active one.

## Context fields

| Field | Purpose |
|---|---|
| `domain` | Domain suffix for manifest substitution and `connect(domain)` URLs |
| `infra_dir` | Path to infrastructure manifests |
| `kube_context` | kubectl context name |
| `acme_email` | Let's Encrypt / cert-manager email |
| `vpn_url` | Optional VPN endpoint |

## Global state

Set once at startup, read everywhere:

```rust,ignore
sdk::config::set_active_context(ctx);
let ctx = sdk::config::active_context();

sdk::kube::set_context("my-cluster");   // kube feature
let name = sdk::kube::context();
```

- **Active context** — `config::set_active_context` / `config::active_context`.
- **Kube context** — `kube::set_context` / `kube::context` (`kube` feature).
- **Apply semaphore** — a global `tokio::sync::Semaphore(2)` in `kube.rs`
  limits concurrent manifest applications to protect single-node k3s
  clusters.

## Domain-derived URLs

The `connect(domain)` constructors on the REST clients derive standard
hostnames from a domain:

| Client | URL |
|---|---|
| `OpenSearchClient` | `https://search.{domain}` |
| `MatrixClient` | `https://messages.{domain}/_matrix/` |
| `LiveKitClient` | `https://livekit.{domain}` |
| `PrometheusClient` | `https://systemmetrics.{domain}/api/v1/` |
| `LokiClient` | `https://systemlogs.{domain}/loki/api/v1/` |
| `GrafanaClient` | `https://metrics.{domain}/api/` |

These constructors create **unauthenticated** clients. For authenticated
usage, build a g2v `Client` yourself (see
[Getting started](getting-started.md#authentication)).
