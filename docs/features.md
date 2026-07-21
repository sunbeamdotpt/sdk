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
| `auth` | `auth` | connectrpc, buffa, g2v ConnectRPC transport, codegen at build time |
| `kanban` | `kanban` | connectrpc, buffa, codegen at build time |
| `wfectl` | `wfectl` | tonic, prost, wfe crates |
| `search` | `search` | g2v client stack |
| `matrix` | `matrix` | g2v client stack |
| `media` | `media` | g2v client stack |
| `monitoring` | `monitoring` | g2v client stack |
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
  `sunbeam_g2v`, `KanbanClient`, and the generated `v1` surface

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
| `connectrpc`, `buffa` | 0.7 | `sunbeam-g2v` 0.5.2 pairs with connectrpc 0.7; 0.8 breaks `ConnectTransport` interop |
| `bollard` | 0.20 | `testcontainers` 0.27.3 requires it |
| RustCrypto crates (`aes-gcm`, `hmac`, `sha2`, `rand`) | 0.10 / 0.12 / 0.10 / 0.8 | the ecosystem moves in lockstep; partial upgrades break trait interop |
