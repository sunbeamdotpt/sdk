---
type: State
title: Current state of sdk
description: What is in flight, what is blocked, what the next session should pick up first.
tags: [state]
timestamp: 2026-07-21T00:00:00Z
---

# State — 2026-07-21

## In flight

- **v3.1.1 released** (tag on mainline): real-boot fixes for the
  `testing::Kanban` orchestrator reported by cli's integration suite
  (agent-mail #25, acked) — NATS readiness waits on stderr, OpenSearch gets
  a host-side `/_cluster/health` poll before dependents, kanban readiness
  is a `/healthz/live` poll instead of the version-dependent log line. Also
  `From<lettre::address::AddressError>`. cli adopted v3.1.0 fully (their
  secrets_ext.rs deleted, 7 direct deps dropped) and will switch their
  kanban suite back to `sdk::testing::Kanban` on v3.1.1.
- **Release mechanics changed**: the sunbeam cargo registry and the
  gitea/tea release stage are gone — `workflows.yaml` is now checkout →
  lint → test-unit → tag, and consumers pin by git tag only. Tags are
  pushed manually alongside the release push (short-circuits the CI tag
  stage either way — harmless, publish/release no longer exist).

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
