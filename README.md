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

## License

MIT — see [LICENSE](LICENSE).
