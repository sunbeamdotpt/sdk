# Sunbeam SDK

Rust SDK for Sunbeam workspace management, Kubernetes manifests, VPN
integration, OpenBao secrets, and WFE workflow orchestration.

---

## Semantic Memory Search (Optional)

If a `sunbeam-memory` MCP server is available in your environment, use it for
codebase search instead of `grep` or `rg`.

1. **Initialize the repository first.** Before searching, ensure this codebase is
   indexed:
   - Call `add_watch_target` with the absolute path to this repository.
   - Wait for indexing to complete, then search.
2. **Prefer semantic search.** Use `search_facts` with natural-language queries
   about behavior, design decisions, known issues, and prior changes.
3. **Store useful findings.** If you discover something future agents should
   remember (a gotcha, invariant, or decision), call `store_fact` with a concise
   note and a source URN when possible.

`sunbeam-memory` is **optional**. If the server is not available, skip these
steps and use `grep` / `rg` / `Read` as usual. Do not fail, stall, or ask the
user to install it.

---

## Technology Stack

- **Language:** Rust 2024 edition
- **Async runtime:** tokio (full features)
- **CLI parser:** clap v4 with derive macros (dependency for consuming binaries)
- **Kubernetes:** kube-rs (client + runtime + websockets), k8s-openapi
- **Workflow engine:** wfe, wfe-core, wfe-sqlite, wfe-yaml, wfe-server-protos
- **TLS/HTTP:** rustls (aws-lc-rs crypto provider), reqwest, tokio-rustls, h2,
  tonic/prost (gRPC)
- **Serialization:** serde, serde_json, serde_yaml
- **Tracing/Logging:** tracing + tracing-subscriber with custom line/json/threaded
  layers
- **Crypto:** rsa, sha2, hmac, blake2, chacha20poly1305, hkdf, base64, rand,
  aes-gcm, argon2, crypto_box, x25519-dalek, rcgen
- **Email:** lettre (SMTP with tokio + rustls)
- **Secrets:** vaultrs (OpenBao / Vault)
- **Networking/VPN:** boringtun, smoltcp, ipnet, zstd
- **Testing:** cargo nextest, wiremock, pretty_assertions, tokio-test

## Package Structure

```
.
├── Cargo.toml              # Single-package manifest (name = "sdk")
├── build.rs                # Embeds lima-sunbeam.yaml, sets git commit + build metadata
├── src/
│   ├── lib.rs              # Module declarations, #![warn(missing_docs)]
│   ├── error.rs            # SunbeamError, Result, ResultExt, bail! macro
│   ├── auth.rs             # OAuth2 / SSO login flow
│   ├── config.rs           # ~/.sunbeam/config.json (Context, active_context global)
│   ├── constants.rs        # Shared constants
│   ├── kube.rs             # kube-rs client init, server-side apply, rollout restart
│   ├── logging/            # Three-mode tracing subscriber (line, json, threaded)
│   ├── logger.rs           # Structured logger with inherited fields
│   ├── manifest_params.rs  # Runtime parameter discovery (--set) and override application
│   ├── manifests.rs        # Kustomize build + domain substitution + namespace filtering + apply
│   ├── openbao.rs          # OpenBao HTTP client
│   ├── profiles/           # Manifest profile system (shortcuts, rules, validation)
│   ├── secrets.rs          # OpenBao init/unseal/seed, VSO secret sync, port-forward
│   ├── vault_keystore.rs   # Vault transit keystore operations
│   ├── vpn/                  # VPN daemon control and environment detection
│   │   ├── cmds.rs           # connect/disconnect/status commands
│   │   └── env.rs            # daemon socket and environment injection
│   ├── kanban/             # Kanban board/project management gRPC client
│   └── wfectl/             # Remote workflow engine gRPC client
├── workflows.yaml          # WFE CI pipeline definition (lint → test-unit → tag → publish → release)
└── lima-sunbeam.yaml       # Lima VM spec for local k3s + Cilium + BuildKit provisioning
```

## Build & Test

```bash
# Build the library
cargo build --release

# Run all tests (requires cargo-nextest)
cargo nextest run --lib

# Lint
cargo clippy --all-targets -- -D warnings

# Format
cargo fmt --all

# Generate docs
cargo doc --no-deps
```

### Test Strategy

- Unit tests live next to the code in `#[cfg(test)]` modules.
- Integration tests that touch a real Kubernetes cluster are avoided.
- Pure-function tests (parsing, catalog building, filtering, serialization
  roundtrips, topological sorts) are preferred.
- The `logging` module has extensive tests for output formatting using a custom
  `TestWriter`.
- `error.rs` tests cover exit codes, display formatting, context extensions, and
  the `bail!` macro.
- `kanban/` service functions are tested behind `mockall::automock` service
  traits; the kanban module targets >90% line coverage via `cargo llvm-cov`.

## Architecture

### Error Handling

Every module returns `Result<T>` (alias for
`std::result::Result<T, SunbeamError>`). `SunbeamError` is a `thiserror` enum
with variants: `Kube`, `Config`, `Network`, `Secrets`, `Build`, `Identity`,
`ExternalTool`, `Io`, `Json`, `Yaml`, `Other`.

- Use `bail!("message")` for early returns with `SunbeamError::Other`.
- Use `.ctx("context")` or `.with_ctx(|| "lazy context".into())` from
  `ResultExt` to add context without losing structured error variants.
- Convenience constructors: `SunbeamError::kube("...")`,
  `SunbeamError::config("...")`, etc.

### Global State

- **Active Context:** Set once at startup via `config::set_active_context(ctx)`
  and read everywhere with `config::active_context()`.
- **Kube Context:** Set similarly via `kube::set_context("...")` and read with
  `kube::context()`.
- **Apply Semaphore:** A global `tokio::sync::Semaphore(2)` in `kube.rs` limits
  concurrent manifest applications to protect single-node k3s clusters.

### Remote Services

- **`kanban/`** — gRPC client for the Sunbeam Kanban service: boards, cards,
  projects, templates, attachments, search, and real-time subscriptions.
- **`wfectl/`** — gRPC client for the WFE workflow engine: list, run, logs,
  cancel, suspend, resume, and publish workflows remotely.

### Configuration

User config is stored in `~/.sunbeam/config.json` (`SunbeamConfig`). It contains
multiple named `Context`s, each with:

- `domain` — domain suffix for manifest substitution
- `infra_dir` — path to infrastructure manifests
- `kube_context` — kubectl context name
- `acme_email` — Let's Encrypt / cert-manager email
- `vpn_url` — optional VPN endpoint

SDK consumers read per-domain config from `~/.sunbeam/config.json`.

### Logging

Three output modes are available through `logging::init_subscriber`:

- `line` (default) — awk-friendly `key="value"` single-line format.
- `json` — NDJSON structured logs.
- `threaded` — grouped concurrent output with per-task scrollback
  (BuildKit-style); falls back to `line` when stderr is not a TTY.

Verbosity is controlled by `RUST_LOG`. The default filter silences noisy
dependencies: `tonic`, `hyper`, `h2`, `tower`, `reqwest`, and kube TLS/builder
noise.

The `logger` module provides a separate structured logger with inherited fields
for SDK consumers.

## Code Style — Follow Existing Patterns Exactly

**Module docstrings:** One-line, starts with a capital letter, uses em-dash to
separate topic from description:

```rust
//! Service management — status, logs, restart.
```

**Imports:** stdlib first, then crates, then `crate::` internals. Group with
blank lines.

**Output/logging:** Library command functions return structured data
(`Result<T>`) and let the caller render it. Use `tracing::info!`,
`tracing::debug!`, etc. with structured fields for diagnostics:

```rust
tracing::info!(msg = "Applying manifests");
tracing::info!(msg = "Namespace created");
tracing::warn!(msg = "Pod not ready");
```

Never use bare `println!` / `eprintln!` for command output in SDK modules.

**Error flow:** `bail!("message")` or
`return Err(SunbeamError::Other("msg".into()))` for fatal errors. Callers
print the error chain and exit with the error's exit code.

**Tracing:** Use `tracing::info!`, `tracing::debug!`, etc. with structured
fields: `tracing::info!(msg = "...", key = %value)`.

**Avoid:**

- Don't add the `log` crate — use `tracing` or `logger.rs` helpers.
- Don't wrap every kube call in `match` / `if let` when `?` + `.ctx()` is
  sufficient.
- Don't create utility modules or shared abstractions for one-off operations.

## CI / CD

Continuous integration is defined in `workflows.yaml` and executed by the WFE
workflow engine.

Pipeline stages:

1. **checkout** — clone and checkout the commit
2. **lint** — `cargo fmt --all -- --check` + `cargo clippy --all-targets -- -D warnings`
3. **test-unit** — `cargo nextest run --lib`
4. **tag** (mainline only) — read version from `Cargo.toml`, create and push a
   Git tag
5. **publish** (tag created only) — `cargo publish -p sdk --registry sunbeam`
6. **release** (tag created only) — create a release via the `tea` CLI

Integration tests that require real services are **not** run in CI.

## Security Considerations

- **Never commit secrets** — no `.env` files, credentials, or keys in the repo.
- **TLS:** The project uses pure rustls (no native-tls). The aws-lc-rs crypto
  provider is expected to be installed by the consuming binary.
- **VPN:** When the VPN daemon is running, `kube::get_client()` rewrites the
  cluster URL to a loopback proxy inside the WireGuard trust boundary and
  disables TLS verification for that hop.
- **Secrets:** OpenBao (HashiCorp Vault fork) is used for KV secrets, database
  engine config, and transit keystore operations. Root tokens are short-lived
  and obtained via port-forward.
- **Authentication:** OAuth2/OIDC via Hydra for SSO; Gitea personal access
  tokens for Git operations.
- **Crypto primitives:** Modern, well-audited crates (chacha20poly1305,
  aes-gcm, argon2, x25519-dalek) are used for encryption, key derivation, and
  VPN tunneling.

## Dependencies

- **Do NOT add unnecessary dependencies.** The package already pulls in
  kube-rs, clap, tokio, wfe, serde, etc. Prefer stdlib or existing deps.
- **Do NOT refactor code you weren't asked to change.** Touch only what the
  task requires.
- **Do NOT over-engineer error handling.** Use existing `SunbeamError` variants
  and `bail!` for early returns.
- **Do NOT create new files** unless absolutely necessary. Prefer editing
  existing modules.

## What NOT to Do

- Don't add the `logging` crate. Use `tracing` or `logger.rs` helpers.
- Don't wrap every kube call in `match` / `if let` when `?` + `.ctx()` is
  sufficient.
- Don't create utility modules or shared abstractions for one-off operations.
