---
title: Feature Flags
description: Cargo feature reference for tree-shaking the Sunbeam SDK.
tags:
  - features
  - tree-shaking
category: reference
nav_order: 2
created_at: "2026-07-20"
related:
  - getting-started.md
  - testing.md
---

# Feature Flags

`default = ["full"]` enables every module — nothing changes for existing
consumers. Tree-shaking is opt-in:

```toml
sdk = { git = "https://github.com/sunbeamdotpt/sdk", default-features = false, features = ["matrix", "openbao"] }
```

The bare core (no features) compiles only `error`, `config`, `constants`,
`logger`, and `logging` — about a fifth of the full dependency graph.

## Reference

| Feature | Modules | Pulls in |
|---------|---------|----------|
| `auth` | `auth` | connectrpc, buffa, the vendored g2v ConnectRPC transport (`g2v-client-connectrpc`), codegen at build time |
| `kanban` | `kanban` | connectrpc, buffa, codegen at build time |
| `wfectl` | `wfectl` | tonic, prost, wfe crates |
| `search` | `search` | implies `g2v-client` |
| `matrix` | `matrix` | implies `g2v-client` |
| `media` | `media` | implies `g2v-client` |
| `monitoring` | `monitoring` | implies `g2v-client` |
| `g2v-client` | `g2v::client` | vendored Sunbeam client stack (reqwest, tower, lru); implied by the REST clients |
| `g2v-server` | `g2v::service`, `g2v::server`, `g2v::middleware`, `g2v::health`, `g2v::metrics`, `g2v::telemetry`, `g2v::config` | vendored axum service runtime (opt-in; NOT part of `full`) |
| `g2v` | both stacks | `g2v-client` + `g2v-server` |
| `g2v-nats` / `g2v-sqlx` / `g2v-redis` / `g2v-vault` / `g2v-election` / `g2v-standalone` / `g2v-client-connectrpc` | respective `g2v` submodules | imply `g2v-server` (or `g2v-client` for the ConnectRPC transport); sqlx is pinned to `tls-rustls-aws-lc-rs` |
| `build` | `build` | — (wraps the host `buildctl` binary) |
| `kube` | `kube`, `manifests`, `manifest_params`, `profiles` | kube-rs, k8s-openapi |
| `openbao` | `openbao` | vaultrs |
| `secrets` | `secrets` | `kube` + `openbao` + rsa/pkcs crypto |
| `vault-keystore` | `vault_keystore` | argon2, aes-gcm |
| `vpn` | `vpn`, VPN hook inside `kube` | sunbeam-net (git) |
| `lettre` | — (`From<lettre>` impls on `SunbeamError`) | lettre (opt-in; not part of `full`) |
| `testing` | `testing` | testcontainers, bollard |

## Public-API dependency re-exports

Several SDK types appear in signatures (`kube::Client`, `reqwest::Error`,
`connectrpc::ConnectError`, `buffa::MessageField`, …). A consumer that adds
its own direct dependency on a *different* version of those crates gets a
second copy in the graph and type mismatches. To avoid version skew, name
them through the SDK instead:

- `sdk::reqwest`, `sdk::kube_rs`, `sdk::k8s_openapi`
- `sdk::kanban::prelude` — re-exports `connectrpc`, `buffa`, `buffa-types`,
  the vendored `sdk::g2v` client stack, `KanbanClient`, and the generated
  `v1` surface

## Interactions to know

- **`secrets` enables `kube` + `openbao`** automatically.
- **`testing::Kanban` requires `auth`:** the kanban stack orchestrator
  provisions its service credentials through the generated sso-gateway IAM
  client, so the module only exists when both features are on.
- **`vpn` augments `kube`:** when both are on, `kube::get_client()` rewrites
  the cluster URL to the loopback proxy inside the WireGuard trust boundary.
  Without `vpn`, the hook compiles out.
- **`build.rs` codegen** for the sso-gateway and Kanban stubs only runs when
  `auth` / `kanban` are enabled — minimal builds skip the BSR download
  entirely.
- **Container tests** next to client modules are gated on
  `all(test, feature = "testing")` *and* their module's feature. To run
  everything: `cargo nextest run --all-features --lib`.

## Intentional dependency pins

Some deps are held back by internal constraints, not neglect:

| Dependency | Pinned at | Reason |
|---|---|---|
| `connectrpc`, `buffa` | 0.7 | the vendored g2v and the generated stubs pair with connectrpc 0.7; 0.8 breaks `ConnectTransport` interop |
| `sunbeam-g2v` (external) | removed in v3.4.0 | the framework is vendored under `src/g2v/`; the upstream repo is deprecated at 0.6.2 |
| `sqlx` (g2v) / `sqlx` (wfe) | 0.9 / 0.8 | vendored g2v uses sqlx 0.9; wfe-sqlite still pins 0.8 — both majors coexist in the graph |
| `bollard` | 0.20 | `testcontainers` 0.27.3 requires it |
| `kube` / `tonic` TLS features | `aws-lc-rs` / `tls-aws-lc` | one TLS crypto backend (aws-lc-sys); kube's defaults select `ring`, tonic's TLS is providerless — see the TLS policy note in `Cargo.toml` |
| RustCrypto crates (`aes-gcm`, `hmac`, `sha2`, `rand`) | 0.10 / 0.12 / 0.10 / 0.8 | the ecosystem moves in lockstep; partial upgrades break trait interop |
