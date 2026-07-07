# Sunbeam SDK

The **Sunbeam SDK** is a Rust library for building Sunbeam-compatible tooling:
workspace management, Kubernetes manifest operations, VPN integration, OpenBao
secrets handling, and WFE workflow orchestration.

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

- `config` — Context-based configuration file I/O.
- `kube` — kube-rs client setup and manifest apply helpers.
- `manifests` — Kustomize build, domain substitution, and namespace filtering.
- `profiles` — Manifest-anchored profile override system.
- `workflows` — WFE workflow definitions, primitives, and step implementations.
- `kanban` — Kanban board/project management gRPC client.
- `secrets` / `openbao` / `vault_keystore` — OpenBao / Vault operations.
- `vpn_cmds` / `vpn_env` — VPN daemon control and environment detection.
- `logger` / `logging` — Structured logging subsystems.

See `src/lib.rs` for the full module list and `AGENTS.md` for project conventions.

## License

MIT — see [LICENSE](LICENSE).
