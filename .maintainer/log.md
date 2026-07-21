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
