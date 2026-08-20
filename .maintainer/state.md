---
type: State
title: Current state of sdk
description: What is in flight, what is blocked, what the next session should pick up first.
tags: [state]
timestamp: 2026-08-20T00:00:00Z
---

# State — 2026-08-20

## In flight

- **SDK-014 (clippy panic/default ban, SSO-027 instance)** — in progress,
  started then paused by the human mid-session; no repo changes made yet
  (surveyed only: ~520 `unwrap`/`expect`/`*_or_default` matches across 50
  files, most in `#[cfg(test)]`). Card has full config spec.
- **v3.3.3 RELEASED** (2026-08-20, release commit `6b663833`, tag `v3.3.3`
  to push) — new `testing::Nats` builder + configurable NATS image in
  `testing::Kanban`. No API changes.
- **v3.3.2 RELEASED** (2026-07-31, release commit `2c5aa642`, tag `v3.3.2`
  pushed) — SDK-012 unit tests + SDK-015 end-to-end coverage +
  `testing::Kanban` identity scopes. Cut so cli can pin it for its next
  cut.
- **v3.3.1 RELEASED** (mainline push 2026-07-31, release commit `8f5ea4fd`;
  tag `v3.3.1` pushed locally — the CI tag stage does not fire on push, the
  tag-only flow is the real mechanism; see log). Patch: SDK-011 test
  coverage only, no API changes.
- Worktree clean; SDK-014 (clippy ban) paused by the human, no repo changes
  made for it.

## Done this cycle

- SDK-011 closed: verified the v2026.07.30 sso-gateway defs (BSR commit
  `a322e5fe`) were already in the generated stubs since v3.2.0 — the ticket
  was confirm-and-lock, not regenerate. Added unit tests (application
  `cross_tenant`/`skip_consent` surface, `BoolValue` partial-update
  presence) and extended `sso_gateway_application_crud` to round-trip +
  toggle `cross_tenant`. 363/363 lib tests, clippy, fmt, and the
  Docker-backed integration test all green.
- SDK-012 closed: KANBAN-035's proto landed on BSR (`string email = 4` on
  `Assignee`); added serde roundtrip + wiremock GetCard-decode unit tests.
  Note: build.rs only re-exports on `.git/HEAD`/lima-yaml change — `touch
  build.rs` to pick up a fresh BSR push.
- SDK-015 closed: end-to-end assignee-email test against kanban
  `v2026.07.12` + sso-gateway pinned `v2026.07.21` — green in ~20s with
  warm images. Debug chain recorded in the log (RootlessKit port
  collisions, gateway:latest crash, scopes, schema seeding, KanbanCard
  object, relation names).
- Filed KANBAN-035 (proto `string email = 4;` + server population +
  `buf push`) after the human's v2026.07.31 BSR push (`22f90346`) turned out
  to cover `Column.is_done` + `AssignCardRequest` email forms but not
  `Assignee.email`.
- Filed SSO-029 (high): `sso-gateway:latest` (2026-07-30) crashes on fresh
  OpenFGA bootstrap — entitlement tuple type missing from model; smoke
  tests pass superficially because readiness is probed before the crash.
- Filed KANBAN-052: proto/doc drift — AssignCard/GetCard check the
  KanbanCard object (not board), AddMember relations are
  admin/editor/viewer, server identity client needs scope `identity:read`.
- Repaired `remote.origin.fetch` refspec — it was pinned to the deleted
  `refactor/remove-sdk` branch, breaking all fetches.

## Blocked / waiting

- wfectl `anyhow` → `SunbeamError` unification (cli #18 item 5): breaking
  change, deferred to the human for the next major.
- `src/vpn/cmds.rs:212` hardcoded value wants making configurable someday;
  not urgent.

## Pick up first

- SDK-014 (clippy ban) — the only open sdk card; human paused it once, so
  confirm before diving in.
- Watch for cli's adoption of v3.3.x (CLI-011) and any fallout from the
  `From<ConnectError>` → `Connect` variant change.
- Verify whether `proto/sunbeam/kanban/v1/` copies are actually unused by
  `build.rs`; if so, propose removal (escalate first — proto layout may be
  contractual for someone). Confirmed stale this cycle (no `Assignee.email`,
  wrong relation names) — see KANBAN-052.
