---
type: State
title: Current state of sdk
description: What is in flight, what is blocked, what the next session should pick up first.
tags: [state]
timestamp: 2026-07-21T00:00:00Z
---

# State — 2026-07-21

## In flight

- **cli v3-migration requests landed on main, untagged** (agent-mail #18/#19/#20,
  all replied + acked): secrets helpers made public (+ new `rand_string_32`,
  all secret generation moved to `OsRng`), opt-in `lettre` feature with the
  two `From` impls, `From<connectrpc::ConnectError>`, `sdk::kanban::prelude`
  re-exports, `KanbanClient::connect`/`with_default_header`,
  `sdk::reqwest`/`sdk::kube_rs`/`sdk::k8s_openapi` re-exports, and the
  `testing::Kanban` full-stack orchestrator (requires `auth`; new
  `SsoGateway::with_network`/`internal_url` + `OpenSearch` network options).
  CHANGELOG "Unreleased" section lists everything. cli drops their
  `secrets_ext.rs` workaround when the tag lands.
- **Waiting on the human: a release.** Charter reserves tagging; when they
  cut one (likely v3.1.0), ping cli (thread #18) to bump and drop workarounds.

## Blocked / waiting

- wfectl `anyhow` → `SunbeamError` unification (cli #18 item 5): breaking
  change, deferred to the human for the next major.
- `src/vpn/cmds.rs:212` hardcoded value wants making configurable someday;
  not urgent.

## Pick up first

- Check for open mail: `agent-mail inbox`.
- Verify whether `proto/sunbeam/kanban/v1/` copies are actually unused by
  `build.rs`; if so, propose removal (escalate first — proto layout may be
  contractual for someone).
- If Docker + ghcr access are available, un-`#[ignore]`-run the new
  `testing::kanban` stack test once to shake out real-boot issues (the
  builder was verified by compile/clippy/unit tests only).
