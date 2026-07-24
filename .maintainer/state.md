---
type: State
title: Current state of sdk
description: What is in flight, what is blocked, what the next session should pick up first.
tags: [state]
timestamp: 2026-07-24T00:00:00Z
---

# State — 2026-07-24

## In flight

- **v3.2.0 released** (tag on mainline): sso-gateway ConnectRPC stubs
  regenerated from BSR HEAD — new `skip_consent` first-party flag on
  `Application` / `CreateApplicationRequest` / `UpdateApplicationRequest`
  (BoolValue toggle), and `UpdateApplicationRequest` is now a documented
  partial update. `testing::Kanban` sets `skip_consent: false` on its m2m
  app. Integration suite covers the flag round-trip + partial-update
  semantics against gateway image `v2026.07.22` (new default;
  `SSO_GATEWAY_IMAGE_TAG` still overrides). 5/5 live stack tests green,
  358/358 unit, clippy/fmt clean.

## Deferred to next cycle (all replied + acked to cli)

- **#39 (production bug)**: `tools::ensure_tool` uses `reqwest::blocking` —
  panics when the tool cache is cold inside async contexts. Non-breaking
  fix: wrap the download in `tokio::task::spawn_blocking`; async-ifying
  `ensure_tool`/`kustomize_build` is cleaner but breaks the public
  signature → next major.
- **#44**: `From<ConnectError>` collapses to `Network { context }` and
  loses the structured `ErrorCode`. Plan: dedicated variant or structured
  code field so consumers match structurally.
- **#33**: enable the Kratos recovery courier in `testing::SsoGateway`;
  add `sso_url` / `sso_client_id` to `config::Context` (additive).

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
