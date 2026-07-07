# Sunbeam SDK

The **Sunbeam SDK** is a Rust library for building Sunbeam-compatible tooling:
remote service clients (Kanban, WFE), authentication, Kubernetes manifest
tunables, VPN integration, and OpenBao secrets handling.

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org/)

## Using the SDK

Add it to your `Cargo.toml`:

```toml
[dependencies]
sdk = { git = "https://github.com/sunbeamdotpt/sdk" }
```

## Building

```bash
cargo build --release
cargo nextest run --lib
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Modules

- `auth` — OAuth2 / SSO token handling.
- `config` — Context-based configuration file I/O.
- `kanban` — Kanban board/project management gRPC client.
- `wfectl` — WFE workflow engine remote gRPC client.
- `kube` — kube-rs client setup and manifest apply helpers.
- `manifests` — Kustomize build, domain substitution, and namespace filtering.
- `manifest_params` — Runtime manifest parameter discovery.
- `profiles` — Manifest-anchored profile override system.
- `secrets` / `openbao` / `vault_keystore` — OpenBao / Vault operations.
- `vpn_cmds` / `vpn_env` — VPN daemon control and environment detection.
- `logger` / `logging` — Structured logging subsystems.

See `src/lib.rs` for the full module list and `AGENTS.md` for project conventions.

## License

MIT — see [LICENSE](LICENSE).
