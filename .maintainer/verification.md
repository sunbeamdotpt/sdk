---
type: Runbook
title: Verifying a change
description: Build, test, lint commands — nextest, the buf prerequisite, and the WFE CI pipeline.
tags: [testing, ci, runbook]
timestamp: 2026-07-20T00:00:00Z
---

# Verifying a change

```bash
cargo build --release                         # needs buf on PATH (auth/kanban codegen)
cargo nextest run --lib                       # unit tests, no Docker
cargo nextest run --lib --features testing    # + container tests (needs Docker socket)
cargo clippy --all-targets -- -D warnings     # also run with --features testing
cargo fmt --all -- --check
cargo doc --no-deps
```

## Gotchas

- **No `buf` on PATH → build panics** in `build.rs`. Check first if a build
  fails weirdly (`command -v buf`).
- The sso-gateway e2e tests are **serialized single-threaded**
  (`.config/nextest.toml`) because concurrent Docker stacks flake on startup.
  Don't parallelize them.
- Container tests need a Docker socket; a missing socket is an environment
  failure, not a code failure.

## CI reality

No GitHub Actions. `workflows.yaml` is a **WFE workflow** run by wfe-server on
push: checkout → lint → test-unit → (mainline only: tag from `Cargo.toml`
version → `cargo publish -p sdk --registry sunbeam` → release). Container
tests are **not run in CI** — lib tests only. So local `nextest` with
`--features testing` is the only place the testing builders get exercised
before consumers find breakage.

Quirks: the checkout step clones into a directory named `cli` (copy-paste
residue, harmless); release uses `tea` against GitHub with a Gitea token
(deliberate per history).

## Releasing

Semver. Bump `Cargo.toml`, hand-written CHANGELOG entry, push to mainline —
CI tags `v<X.Y.Z>` and publishes. **Releases are the human's call** (a push
with a bumped version *is* a publish). See [charter.md](charter.md).
