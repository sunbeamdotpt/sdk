---
type: Reference
title: Interfaces with other systems
description: Who consumes sdk (the testing contract), what sdk depends on, and the sunbeam-sdk naming trap.
tags: [interfaces, cross-repo]
timestamp: 2026-07-20T00:00:00Z
---

# Interfaces

## Consumers — the contract that matters most

`kanban`, `nats-callout`, and `proxy` all depend on:

```toml
sdk = { git = "…/sdk", tag = "v3.0.0", default-features = false, features = ["testing"] }
```

Their test suites are built on `src/testing/` container builders. **Renaming
or removing a builder breaks three repos** — escalate and file a heads-up
card on each consumer's project board before doing it (charter). Other
modules are available to consumers but currently only `testing` is pinned
by tag.

## The naming trap: `sdk` vs `sunbeam-sdk`

Two different crates with near-identical names:

- **`sdk`** (this repo, v3.0.0) — the pure library, git-dep + private registry.
- **`sunbeam-sdk`** (in-tree crate of the sibling `cli` repo, v2.0.0-rc5) —
  the old 1.x/2.0 CLI+SDK lineage, path dependency of the `sunbeam` binary.

The `cli` repo does **not** depend on this repo. When mail or docs say "the
sdk", check context before acting — and never "reunify" them; that's a human
call.

## Dependencies (Sunbeam-internal)

- `sunbeam-g2v` 0.5.2 (sibling `g2v` repo) — the REST client stack; pins
  connectrpc/buffa 0.7 and requires `auth`+`tracing`+`logging` features even
  for client-only builds
- `sunbeam-net` (sibling `vpn` repo, optional) — VPN transport
- wfe crate family 1.10 — workflow engine client
- BSR modules `buf.build/sunbeamdotpt/sso-gateway` and `/kanban` — codegen
  sources; sources of truth live in those repos
- `~/.sunbeam/config.json` contexts — shared config contract with the CLI

## Runtime and CI ties

Talks to: k3s clusters, OpenBao, sso-gateway, Kanban, WFE server, OpenSearch,
Matrix, LiveKit, monitoring stack, BuildKit. CI (`workflows.yaml`) expects
wfe-server deployed via the sbbb infra repo (`base/wfe/`) plus a
`wfe-credentials` cluster secret — changes to that contract are a task for
the `sbbb` identity.
