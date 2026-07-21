---
type: State
title: Current state of sdk
description: What is in flight, what is blocked, what the next session should pick up first.
tags: [state]
timestamp: 2026-07-20T00:00:00Z
---

# State — 2026-07-20

## In flight

- **v3.0.0 released today** (2026-07-20): CLI removed (lives on as `cli`'s
  in-tree `sunbeam-sdk`), auth replaced with sso-gateway ConnectRPC
  `AuthClient`, kanban wrappers replaced by thin client, per-module features
  introduced (`default = ["full"]`). Consumers pin `tag = "v3.0.0"`.
- **Maintainer system bootstrap**: bundle created 2026-07-20; sdk is the third
  repo enrolled in agent-mail (after sbbb, kanban).

## Blocked / waiting

- Nothing external. The `src/vpn/cmds.rs:212` hardcoded value wants making
  configurable someday; not urgent.

## Pick up first

- Check for open mail: `agent-mail inbox`.
- Stale-docs sweep: `AGENTS.md` tech-stack section lists removed crates
  (lettre, boringtun, smoltcp, …) — see [known-issues.md](known-issues.md).
- Verify whether `proto/sunbeam/kanban/v1/` copies are actually unused by
  `build.rs`; if so, propose removal (escalate first — proto layout may be
  contractual for someone).
