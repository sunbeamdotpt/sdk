---
type: State
title: Current state of sdk
description: What is in flight, what is blocked, what the next session should pick up first.
tags: [state]
timestamp: 2026-07-24T00:00:00Z
---

# State — 2026-07-24 (evening)

## In flight

- **v3.3.0 RELEASED** (tag pushed 2026-07-24): #39 ensure_tool thread fix,
  #44 `SunbeamError::Connect` variant, #33 recovery courier +
  `sso_url`/`sso_client_id` Context fields, kanban `labels()`/`milestones()`
  accessors. cli notified on threads #39/#44/#33; CLI-011 (cli's adoption
  task) updated with the release note.

## Done this cycle

- SDK-001/002/003 fixed and moved to done with notes; SDK-010 filed + done
  (kanban labels/milestones).
- Misfiled cards rehomed per charter: sso-gateway API gaps → sso project
  (SSO-005..009), cli adoption task → cli project (CLI-011, blocked on the
  next sdk tag).
- Mail threads #39/#44/#33 replied, all four inbox messages acked; inbox
  zero. cli knows workarounds must stay until the tag lands.

## Blocked / waiting

- wfectl `anyhow` → `SunbeamError` unification (cli #18 item 5): breaking
  change, deferred to the human for the next major.
- `src/vpn/cmds.rs:212` hardcoded value wants making configurable someday;
  not urgent.

## Pick up first

- Watch for cli's adoption of v3.3.0 (CLI-011) and any fallout from the
  `From<ConnectError>` → `Connect` variant change.
- Verify whether `proto/sunbeam/kanban/v1/` copies are actually unused by
  `build.rs`; if so, propose removal (escalate first — proto layout may be
  contractual for someone).
