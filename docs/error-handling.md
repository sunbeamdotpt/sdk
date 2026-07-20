---
title: Error Handling
description: SunbeamError variants, ResultExt context, and the bail! macro.
tags:
  - errors
  - reference
category: reference
nav_order: 12
created_at: "2026-07-20"
related:
  - getting-started.md
  - configuration.md
---

# Error Handling

Every module returns `sdk::error::Result<T>` —
`std::result::Result<T, SunbeamError>`. `SunbeamError` is a `thiserror` enum
whose variants map to logical categories, each with its own process exit code
for CLI consumers.

## Variants

| Variant | Category | Exit code |
|---|---|---|
| `Kube` | Kubernetes API / cluster | 3 |
| `Config` | Missing/invalid configuration | 4 |
| `Network` | HTTP and transport errors | 5 |
| `Secrets` | OpenBao / Vault | 6 |
| `Build` | Image builds | 7 |
| `Identity` | Identity / user management | 8 |
| `ExternalTool` | kustomize, buildctl, etc. | 9 |
| `Io`, `Json`, `Yaml`, `Other` | General | 1 |

## Adding context

Use `ResultExt` to annotate errors without losing the structured variant:

```rust,ignore
use sdk::error::ResultExt;

let client = kube::get_client()
    .await
    .ctx("loading cluster client")?;

let token = read_token(path)
    .await
    .with_ctx(|| format!("reading token from {path}"))?;
```

`.ctx("...")` takes a static message; `.with_ctx(|| ...)` builds it lazily.

## Early returns

```rust,ignore
use sdk::bail;

if pods.is_empty() {
    bail!("no pods found in namespace {ns}");
}
```

`bail!` returns `SunbeamError::Other`.

## Convenience constructors

```rust,ignore
SunbeamError::kube("...")       // Kube { context, source: None }
SunbeamError::config("...")
SunbeamError::network("...")
SunbeamError::secrets("...")
SunbeamError::build("...")
SunbeamError::identity("...")
SunbeamError::tool("kubectl", "exit 1: connection refused")
```

## `From` conversions

Leaf errors convert automatically with `?`: `reqwest::Error`, `io::Error`,
`serde_json`/`serde_yaml`, `base64::DecodeError`, and — gated on their
features — `kube::Error` (`kube`), `tonic` errors (`wfectl`), and
`sunbeam_g2v::client::ClientError` (any g2v-based client feature).

## Exit codes

```rust,ignore
let code = err.exit_code();  // map a SunbeamError to a process exit code
```
