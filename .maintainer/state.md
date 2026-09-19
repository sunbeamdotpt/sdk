---
type: State
title: Current state of sdk
description: What is in flight, what is blocked, what the next session should pick up first.
tags: [state]
timestamp: 2026-09-19T00:00:00Z
---

# State — 2026-09-19

## In flight

- **v3.4.0 RELEASED (2026-09-19)** — three bodies of work, all gates green:
  1. **TLS crypto backends unified on aws-lc-sys** — kube 4.0
     `default-features = false` + `aws-lc-rs` (its default selects ring),
     tonic `tls-aws-lc` (tonic TLS is providerless), testcontainers
     `default-features = false` + `["blocking", "aws-lc-rs"]`. ring remains
     only transitively (wfe -> kube 3.1/sqlx, boringtun in sunbeam-net) —
     WFE-003 filed on wfe's board for the mirror flips.
  2. **g2v vendored into the sdk** (the session's headline): sunbeam-g2v
     0.6.2 (upstream's final release, repo deprecated) absorbed as
     `src/g2v/` with `g2v-client` (implied by search/matrix/media/
     monitoring; in `full`) and `g2v-server` (opt-in, NOT in `full`)
     features, plus granular `g2v-*` passthroughs. sdk is now a 2-crate
     workspace (g2v-derive). Circular g2v<->sdk dev-dep dissolved. g2v's 15
     integration tests + example ported; typescript/ TS client moved here
     and committed.
  3. **SDK-014 closed** — SSO-027 panic/default ban enforced via
     workspace lints + clippy.toml; ~35 production violations fixed with
     explicit handling + logging; generated stubs exempt at include sites.
     `tests/g2v_support` carries the pattern from g2v upstream.
- **Remote-daemon testing**: `testing::init_docker_host` +
  `testing::util::build_image` (bollard classic builder, stream drained)
  make container tests work against remote TLS Docker contexts. Suite ran
  500/500 (3 skipped) against `alpha-0`; all suite images pre-pulled there
  with the human's authenticated CLI after Docker Hub 429s. Registry auth
  was NOT installed on alpha-0's account (keychain needs interactive
  approval) — the human has the one-liner if they want it.
- **Container-test gotcha**: rustls panics in test processes when both
  provider features are unified and nothing installs a default — the init
  helper handles it; don't remove that call from container tests.

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

- Watch consumer migrations from the g2v deprecation: kanban (client),
  sso-gateway (server), nats-callout (0.3) — cards filed on 2026-09-19.
- Run a proper cargo-llvm-cov pass; the 90% org bar is still unmet
  (61% baseline documented across recent trains).
- The remote-daemon container suite depends on the pre-pulled images on
  alpha-0; future NEW image pulls from testcontainers are anonymous (429
  risk) until registry auth is resident there — one-liner handed to the
  human (keychain needs interactive approval).
- Verify whether `proto/sunbeam/kanban/v1/` copies are actually unused by
  `build.rs`; if so, propose removal (escalate first — proto layout may be
  contractual for someone). Confirmed stale this cycle (no `Assignee.email`,
  wrong relation names) — see KANBAN-052.
