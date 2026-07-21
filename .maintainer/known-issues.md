---
type: KnownIssue
title: Known issues and doc drift
description: Stale docs, repo residue, pinned dependencies, and small debt. Remove entries as fixed.
tags: [known-issue, docs, tech-debt]
timestamp: 2026-07-20T00:00:00Z
---

# Known issues

Active discrepancies and debt. When you fix one, delete the entry here and
note it in [log.md](log.md). Verify against the repo before trusting an entry.

## Stale docs

- `AGENTS.md` tech-stack section lists **removed crates** (lettre, boringtun,
  smoltcp, crypto_box, x25519-dalek, rcgen, blake2, hkdf) — v3.0.0 dropped
  them (VPN moved to `sunbeam-net`). Rest of AGENTS.md is accurate.
- `AGENTS.md` lists `src/auth.rs` — it's `src/auth/mod.rs`.
- Older CHANGELOG entries (1.x) are commit-hash lists; v3.0.0 onward is
  prose. Historical, not worth rewriting.

## Repo residue

- `proto/sunbeam/kanban/v1/*.proto` — local copies of kanban protos appear
  **unused** by `build.rs` (stubs come from BSR; only Keto protos are used as
  an include root). Verify, then propose removal — escalate first.
- `.claude/scheduled_tasks.lock` is tracked in git — local-tool artifact,
  almost certainly accidental. Candidate for removal + gitignore.
- `workflows.yaml` checkout clones into a directory named `cli` (copy-paste
  residue; works, confusing).

## Pinned dependencies — do not "fix"

- connectrpc/buffa 0.7 (pairs with sunbeam-g2v 0.5.2)
- bollard 0.20 + testcontainers 0.27.3 (paired)
- RustCrypto line (aes-gcm/hmac/sha2/rand)
Each pin has a `Cargo.toml` comment. Charter hard rule 3.

## Small TODOs

- `src/vpn/cmds.rs:212` — hardcoded value, TODO to make configurable.
