---
type: State
title: Current state of sdk
description: What is in flight, what is blocked, what the next session should pick up first.
tags: [state]
timestamp: 2026-09-28T00:00:00Z
---

# State — 2026-09-28

## In flight

- **v3.5.0 RELEASED (2026-09-28)** — `iapi` client + connectrpc 0.9.
  Work in `98f39129`, release commit `f8778f97`, lightweight tag
  `v3.5.0`, pushed by hand (mainline + tag). Consumer heads-up cards:
  KANBAN-067, PROXY-009, IAPI-014, SDK-023 (nats-callout — no board,
  filed here). Drift watch: re-vendor card on the sdk dev board for
  `proto/sunbeam/iapi` (no BSR module upstream). Docker-backed suites
  compiled clean but were NOT run this session — schedule a
  `--features testing` container run (and the sso_gateway_client stack
  test) at the next convenience. Body of work, as released:
  1. **`sdk::iapi`** — new `iapi` feature (in `full`) with `IapiClient`,
     ConnectRPC accessors for all eleven `sunbeam.iapi.v1` services
     (regions, networks, VMs, disks, images, blueprints, clusters,
     ledger, DNS, audit, rehydrate). Stubs generated at build time from
     protos **vendored** under `proto/sunbeam/iapi` — iapi keeps its buf
     module local, no BSR.
  2. **connectrpc 0.7 → 0.9** (human-directed in-session):
     runtime 0.9.1 / connectrpc-build 0.9.0 / buffa+buffa-types 0.9.2.
     The old 0.7 pin's reason (0.8 broke AuthClient ConnectTransport
     interop) does NOT extend to 0.9: 480/480 lib tests, all in-process
     g2v integration tests green (client e2e incl. unary + streaming
     over a real axum server). Mechanical breaks fixed in hand-written
     test code: `MessageField` now pins an `Inline<T>` marker
     (`tests/sso_gateway_client.rs`), `StreamMessage::reborrow` became
     `view()` (`tests/g2v_client_e2e.rs`); 0.9 removed the `streaming`
     cargo feature (dropped from `g2v-server`). Pin comments +
     charter rule 3 updated: the 0.9 line moves as a set. Consumers with
     direct connectrpc/buffa deps must align; prelude consumers track
     automatically.
- **v3.4.1 RELEASED (2026-09-28)** — MinIO→RustFS swap shipped:
  `testing::Kanban` defaults to `rustfs/rustfs:1.0.0` (no `1.0` float tag
  exists yet — recheck when 1.0.x ships). Non-breaking: `S3_PORT`/
  `with_s3_tag` added; `MINIO_PORT`/`with_minio_tag` kept as aliases.
  Tag `v3.4.1` cut **by hand** on `48929b5f` — see the no-CI correction
  below. Still open on the kanban side: their own
  `test_support/containers.rs` pins the same dead MinIO tag (KANBAN-066).
- **No CI — correction landed (2026-09-28)** — the "WFE runs
  `workflows.yaml`" story in AGENTS.md/charter was stale; the repo has no
  CI at all (origin github.com, nothing listens). `workflows.yaml`
  deleted; AGENTS.md CI/CD → Releases (local gates, hand-cut tag);
  Gitea-PAT security line dropped; charter updated. Fleet-product Gitea
  docs/code (sunbeam-up, service-discovery-labels, kube.rs, profiles)
  intentionally untouched — that's the deployed Gitea service, not CI.
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

- Bumps for v3.4.1 tracked on cards: KANBAN-066 (kanban's own harness),
  PROXY-008 (proxy), SDK-020 (nats-callout). Kanban's fix is the real
  work — sdk's side is done.
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
