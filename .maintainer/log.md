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

## 2026-07-24 — v3.2.0: sso-gateway `skip_consent` regen

Human reported an auth proto update; BSR diff (2026-07-16 → 2026-07-24
commits) showed exactly one delta: `skip_consent` first-party flag on
`Application` (13), `CreateApplicationRequest` (8), and
`UpdateApplicationRequest` (9, `google.protobuf.BoolValue`), plus docs
making `UpdateApplicationRequest` explicitly a partial update (zero-valued
fields keep stored values). Regeneration is automatic — `build.rs` exports
BSR HEAD, nothing in-repo pins the module — so "updating the protos"
meant fixing compile breakage and covering the new surface:

- `testing::Kanban`'s `provision_service_app` constructs
  `CreateApplicationRequest` exhaustively (no `..Default::default()`), so
  any new request field is a compile error there by design — it now sets
  `skip_consent: false` (m2m client, no browser flow).
- Integration suite round-trips the flag and asserts partial-update
  semantics. Gateway image default bumped `v2026.07.20` → `v2026.07.22`
  (verified via ghcr that v2026.07.22/latest were built 4 minutes after
  the proto commit, so they implement the flag). Lesson: a BSR proto bump
  without a matching gateway image bump makes integration tests fail
  server-side — always confirm the image predates nothing.
- BSR commit archaeology: to find "what changed since we last built" with
  no in-repo pin, diff `buf export` of recent `buf registry module commit
  list` entries and match against fields the code already references
  (`cross_tenant` usage proved the last pull was ≥ 2026-07-16T12:57).

Mail: #35 acked (clean bill); #39 (reqwest::blocking panic — real
production bug, spawn_blocking fix queued), #44 (structured ConnectError
code), #33 (recovery courier + sso config fields) replied and deferred to
the next cycle — kept v3.2.0 scoped to the proto update per the human's
request. Release: two commits + manually pushed `v3.2.0` tag per the
tag-only flow, approved in-session.

## 2026-07-24 — agent-mail → kanban ticketing migration

Cross-repo coordination moved off agent-mail (deprecated) onto kanban cards
via `sunbeam kanban` — the same migration sbbb did earlier. The AGENTS.md
ritual, charter, state.md, and interfaces.md now describe the kanban flow;
mail references in older entries (#18/#19/#20, replies #88–#90) are
historical identifiers, kept so the record stays traceable. *Why:* the
human standardized cross-repo tracking on kanban so tickets are visible to
everyone, not just the two mail endpoints.

## 2026-07-24 — Deferred cli batch (#39/#44/#33) + kanban client refresh

Picked up the three items deferred from the v3.2.0 cycle, all on mainline,
untagged (releases are the human's call):

- **#39 ensure_tool panic**: fixed with a dedicated OS thread for the
  download, not `spawn_blocking`. *Why:* `ensure_tool` is sync and must stay
  sync until the next major; `spawn_blocking` needs a runtime handle and
  only moves the problem — the reqwest::blocking runtime would still be
  dropped from a thread the caller's runtime knows about. A plain
  `std::thread::spawn` + `join` is runtime-agnostic and the blocking client
  lives and dies entirely outside any async context. Regression test runs
  `ensure_kustomize` inside `Runtime::block_on`.
- **#44 structured ConnectError code**: new `SunbeamError::Connect { code,
  context }` variant behind `auth`/`kanban`, exit code still NETWORK,
  Display unchanged. Chose a variant over a `code` field on `Network`
  because every `Network` construction site uses struct-literal syntax — a
  new field would touch them all, and connectrpc isn't compiled in most
  feature combos. Noted in the changelog that match arms on `Network` for
  ConnectRPC failures must move (cli string-matches today; Display is
  unchanged so nothing breaks at runtime).
- **#33 recovery courier + config fields**: `testing::SsoGateway` enables
  the Kratos recovery flow + courier with a default dead-end SMTP URI
  (mirrors `sso-gateway/deploy/kratos.yml`), overridable via
  `with_kratos_courier_smtp` so tests that start stalwart can get real
  delivery. `config::Context` gained additive `sso_url` / `sso_client_id`.
- **Kanban client**: human asked mid-session to "update the kanban client".
  BSR HEAD had gained `LabelService` and `MilestoneService` (stubs regen
  automatically via `buf export` in build.rs); the handwritten wrapper
  lacked accessors. Added `KanbanClient::labels()` / `::milestones()`.
  `events.proto` is messages-only, no service. Card SDK-010.

Ticketing hygiene: SDK-004..008 were sso-gateway API gaps misfiled on sdk's
dev board and SDK-009 was cli's adoption task — the charter says file on
the owning team's project board, and both `sso` and `cli` projects exist.
Refiled as SSO-005..009 and CLI-011 (descriptions carry the old refs) and
deleted the sdk copies. SDK-001..003 moved to done with fix notes.

Validation: fmt clean, clippy clean (default + `testing`), 361/361 unit
tests, courier substitution tests green. Replied on mail threads #39/#44/#33
and acked all four open messages; inbox zero.

## 2026-07-24 — v3.3.0 prepped (minor, not major)

Human approved release prep in-session. Version call: **3.3.0, not 4.0.0**,
despite `From<ConnectError>` now yielding a new `Connect` variant.
Reasoning: Display output and exit codes are unchanged, so nothing breaks
at runtime; the only compile-level hazard is an exhaustive `match` on
`SunbeamError`, and the three tag-pinned consumers (kanban, nats-callout,
proxy) use the `testing` builders, not error matching; cli vendors its own
v2 crate. The changelog carries the migration note. Precedent: v3.1.0
shipped similar additive-with-notes changes as a minor. Commits: feature
batch + `chore(release): 3.3.0`, tag `v3.3.0` created locally on the
release commit per the tag-only flow — push held for explicit confirmation
because pushing the tag *is* the release.

## 2026-07-24 — v3.3.0 pushed

Human confirmed the push in-session. `mainline` + `v3.3.0` pushed to
origin; CLI-011 updated with the release note (bump tag, drop prewarm +
string match, un-ignore device-poll test), SDK-001/002/003/010 descriptions
stamped "Released in sdk v3.3.0", and cli notified on mail threads
#39/#44/#33 (replies on open inbound threads, not new outbound tickets —
the kanban rule covers ticket filing, not thread replies).

## 2026-07-31 — SDK-011 verify-and-lock, v3.3.1 patch

- **SDK-011 ("regenerate from v2026.07.30") was already satisfied.** Checked
  BSR history instead of assuming staleness: commit `a322e5fe` (Jul 30, the
  "v2026.07.30" push) is proto-identical to `183afee5` (Jul 24);
  `cross_tenant` predates Jul 16; `skip_consent` + `UpdateApplication`
  partial-merge landed Jul 24 — all present since v3.2.0's regen, and stubs
  are build-time generated from BSR latest so there is nothing to commit.
  Resolution: lock the surface with tests (unit: field presence + BoolValue
  unset/set semantics; integration: cross_tenant create/get/toggle against
  the real stack), close the card. *Why:* a "regenerate" ticket on a
  build-time-codegen repo is really a "prove we're current" ticket.
- **No BSR pinning.** Considered pinning build.rs to the sso-gateway commit
  for reproducibility; declined — the org's model is floating-latest for
  both modules, and pinning one changes the maintenance contract. Revisit
  deliberately, not as a side effect.
- **SDK-012 rehomed as KANBAN-035.** The Assignee.email gap is a kanban-repo
  proto+server change (charter: proto sources of truth live there). Human
  believed the defs were pushed; the v2026.07.31 push (`22f90346`) actually
  carried `Column.is_done` and email-accepting `AssignCardRequest` — not the
  field. Verified by exporting both commits and diffing. Did NOT set the
  card's `--blocked` flag: the CLI warns it cannot be cleared server-side
  yet; the `depends_on` link expresses the blockage safely.
- **v3.3.1 pushed as a patch** on human's go-ahead; SDK-012 deferred rather
  than holding the train. Unlike v3.3.0 (local tag), this push relies on the
  workflows.yaml tag stage on mainline.
- **Repo config repair:** `remote.origin.fetch` was
  `+refs/heads/refactor/remove-sdk:...` (deleted branch) — every fetch
  failed. Reset to the standard wildcard refspec. Suspect a past session
  narrowed it for a one-off fetch.

Correction to the v3.3.1 entry above: the workflows.yaml tag stage does NOT
fire on push — `v3.2.0`/`v3.3.0` are lightweight local tags, not CI
annotations. The tag-only flow is the actual release mechanism: create the
tag locally on the release commit, push it. `v3.3.1` was tagged locally on
`8f5ea4fd` and pushed ~20 min after the mainline push once the missing tag
was noticed.
