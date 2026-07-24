---
type: State
title: Current state of sdk
description: What is in flight, what is blocked, what the next session should pick up first.
tags: [state]
timestamp: 2026-07-24T00:00:00Z
---

# State — 2026-07-24 (evening)

## In flight

- **v3.3.0 prepped on mainline** (release commit + local `v3.3.0` tag, NOT
  yet pushed): #39 ensure_tool thread fix, #44 `SunbeamError::Connect`
  variant, #33 recovery courier + `sso_url`/`sso_client_id` Context fields,
  kanban `labels()`/`milestones()` accessors. Pushing the tag *is* the
  release — confirm with the human before `git push origin mainline v3.3.0`.

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

- Push `mainline` + `v3.3.0` once the human confirms (tag push = release;
  CI's tag stage is short-circuited by the manual tag per the tag-only
  flow). Then notify cli — CLI-011 unblocks.
- Verify whether `proto/sunbeam/kanban/v1/` copies are actually unused by
  `build.rs`; if so, propose removal (escalate first — proto layout may be
  contractual for someone).
