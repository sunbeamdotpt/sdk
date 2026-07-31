---
type: State
title: Current state of sdk
description: What is in flight, what is blocked, what the next session should pick up first.
tags: [state]
timestamp: 2026-07-31T00:00:00Z
---

# State — 2026-07-31

## In flight

- **v3.3.1 RELEASED** (mainline push 2026-07-31, commit `8f5ea4fd`; CI tags
  from `Cargo.toml`). Patch: SDK-011 test coverage only, no API changes.
- **SDK-012 (kanban Assignee email)** blocked on **KANBAN-035** (filed on
  kanban's dev board, linked `depends_on`). The proto source of truth is the
  kanban repo. Once `email` lands on `buf.build/sunbeamdotpt/kanban`: sdk
  regeneration is automatic (build-time `buf export`); add a unit test
  asserting `Assignee.email` and close SDK-012.

## Done this cycle

- SDK-011 closed: verified the v2026.07.30 sso-gateway defs (BSR commit
  `a322e5fe`) were already in the generated stubs since v3.2.0 — the ticket
  was confirm-and-lock, not regenerate. Added unit tests (application
  `cross_tenant`/`skip_consent` surface, `BoolValue` partial-update
  presence) and extended `sso_gateway_application_crud` to round-trip +
  toggle `cross_tenant`. 363/363 lib tests, clippy, fmt, and the
  Docker-backed integration test all green.
- Filed KANBAN-035 (proto `string email = 4;` + server population +
  `buf push`) after the human's v2026.07.31 BSR push (`22f90346`) turned out
  to cover `Column.is_done` + `AssignCardRequest` email forms but not
  `Assignee.email`.
- Repaired `remote.origin.fetch` refspec — it was pinned to the deleted
  `refactor/remove-sdk` branch, breaking all fetches.

## Blocked / waiting

- SDK-012 → KANBAN-035 (see above).
- wfectl `anyhow` → `SunbeamError` unification (cli #18 item 5): breaking
  change, deferred to the human for the next major.
- `src/vpn/cmds.rs:212` hardcoded value wants making configurable someday;
  not urgent.

## Pick up first

- When the kanban side ships KANBAN-035: sdk-side unit test + close SDK-012.
- Watch for cli's adoption of v3.3.x (CLI-011) and any fallout from the
  `From<ConnectError>` → `Connect` variant change.
- Verify whether `proto/sunbeam/kanban/v1/` copies are actually unused by
  `build.rs`; if so, propose removal (escalate first — proto layout may be
  contractual for someone).
