---
type: State
title: Current state of sdk
description: What is in flight, what is blocked, what the next session should pick up first.
tags: [state]
timestamp: 2026-07-21T00:00:00Z
---

# State — 2026-07-21

## In flight

- **v3.1.0 released** (tag on mainline): the cli v3-migration batch
  (agent-mail #18/#19/#20, all replied + acked). cli was notified on thread
  #18 to bump their pin and drop the workarounds.
- **Release mechanics changed**: the sunbeam cargo registry and the
  gitea/tea release stage are gone — `workflows.yaml` is now checkout →
  lint → test-unit → tag, and consumers pin by git tag only. v3.1.0's tag
  was pushed manually to short-circuit the old pipeline; future releases
  can let the CI tag stage do it (it reads the version from `Cargo.toml`).

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
