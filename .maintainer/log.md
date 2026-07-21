# Decision log

Append-only. Newest at the bottom. Every entry: what was decided, and *why* —
future sessions need the reasoning, not just the outcome. Never rewrite history.

## 2026-07-20 — Maintainer bundle created

Enrolled sdk as the third repo in agent-mail. Two charter decisions worth
recording: (1) the `src/testing/` container builders are treated as the
crate's most load-bearing API because three sibling repos pin them by tag —
breaking them escalates and fans out to consumer repos via mail; (2) pinned
dependency pairs are charter-level, not folklore — connectrpc/buffa 0.7 pairs
with sunbeam-g2v 0.5.2, bollard pairs with testcontainers, and a maintainer
agent doing a well-meaning `cargo update` is a realistic failure mode worth
forbidding explicitly.

## 2026-07-21 — cli v3-migration batch (agent-mail #18/#19/#20)

Did the whole cli request batch as one additive change set; replied and acked
all three threads. Decisions worth recording:

- **`rand_string_32` was requested as if it existed — it didn't.** v3.0.0
  never had it (cli ported it from the old in-tree crate). Added it, and the
  human interrupted mid-implementation to demand CSPRNG: all secret
  generation in `secrets.rs` now uses `rand::rngs::OsRng` (previously
  `thread_rng`), and the charset mapping rejection-samples instead of
  `% 62`-ing raw bytes. Rationale: this module generates real production
  secrets (fernet keys, DKIM, passwords) — treat any "it's just a helper"
  RNG as a bug.
- **lettre came back, but opt-in.** The request claimed "lettre is already an
  sdk dep" — false, v3.0.0 dropped it as dead. Re-adding it unconditionally
  would tax every consumer for two `From` impls, so it's a standalone
  `lettre` feature outside `full`. Precedent: error-conversion dependencies
  are opt-in features, not core.
- **`pub use kube` is impossible** — the SDK has its own `kube` module.
  Exported as `sdk::kube_rs` instead; documented in README/docs/features.md.
- **The `testing::Kanban` orchestrator gates on `auth`** because IAM
  provisioning (tenant + app + rotate-secret) reuses the generated
  sso-gateway client instead of vendoring protos. Shared-network wiring
  required new `SsoGateway::with_network` + `internal_url` — additive, the
  three pinned consumers are unaffected.
- **MinIO bucket creation is hand-rolled SigV4** (hmac/sha2/chrono/reqwest,
  all already deps) rather than pulling an S3 SDK for one PUT. If more S3
  operations are ever needed, revisit — don't extend the by-hand signer.
- **wfectl anyhow→SunbeamError unification deferred**: breaking change,
  release-gated, human's call.
- Housekeeping: fixed the AGENTS.md stale tech-stack/`src/auth.rs` entries
  from known-issues.md (lettre line now describes the opt-in feature) and
  removed those known-issue entries.

## 2026-07-21 — v3.1.0 released; registry/gitea stages removed

Cut v3.1.0: feature batch commit + `chore(release): 3.1.0` pushed to
mainline. Mid-release the human declared the sunbeam cargo registry and
gitea/tea release step dead infrastructure ("we don't need any of the
sunbeam registries or gitea at all"), so the tag was pushed manually
(`v3.1.0` on the release commit) — which also short-circuits CI's
tag→publish→release chain via its `tag_already_existed` check — and the
publish + release stages were stripped from `workflows.yaml` (pipeline is
now checkout → lint → test-unit → tag). Charter and AGENTS.md updated to
match. Recorded because the charter previously said "a version bump *is* a
publish to the private registry" — that is no longer true; a version bump
on mainline is just a git tag now.

## 2026-07-21 — v3.1.1: real-boot fixes for testing::Kanban

cli booted the v3.1.0 orchestrator for real and found two deterministic
bugs plus a shaky wait (mail #25). Fixes, and the reasoning:

- **nats-server logs readiness on stderr** across all current tags. The
  stdout log wait could never fire. Field report > source reading — I had
  copied the wait message from kanban's own harness, which polls TCP from
  the host instead of using a log wait, so the wrong stream never showed
  up there either.
- **OpenSearch had no readiness wait** and the kanban server does not
  retry system migrations — deterministic connection-refused on the
  backfill step. Fixed orchestrator-side (publish port + host poll for
  green/yellow) rather than in the shared `OpenSearch` builder: changing
  the shared builder's wait behavior would touch the three pinned
  consumers' suites for no benefit they asked for. Orchestrator-local
  fixes first; promote to the shared builder only when a second
  orchestrator needs it.
- **"kanban listening" vs "kanban service starting"**: both exist in
  kanban source, but the latter fires *before* migrations and the former
  isn't in older published images. Log waits against a versioned,
  externally-built image are fragile — the orchestrator now polls
  `/healthz/live` from the host. Principle: readiness of foreign images
  goes through their HTTP health endpoints, not their log text.
- v3.1.1 tagged manually per the new tag-only release flow; human
  approved the tag in-session. All three cli threads (#25/#26/#27)
  replied and acked.
