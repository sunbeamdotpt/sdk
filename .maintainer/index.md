# sdk maintainer knowledge bundle

OKF-shaped knowledge base for the sdk maintainer. Start here, follow links.
Instructions live in [charter.md](charter.md); code conventions live in the
repo's `AGENTS.md`; this bundle holds *knowledge* — what is true and why.

## State

- [state.md](state.md) — what is in flight right now, updated at every handoff
- [log.md](log.md) — append-only decision journal

## Concepts

- [architecture.md](architecture.md) — what the crate provides and how it's feature-gated
- [verification.md](verification.md) — build/test/CI: nextest, the buf gotcha, WFE pipeline
- [interfaces.md](interfaces.md) — who consumes sdk (the testing contract) and what it depends on
- [known-issues.md](known-issues.md) — stale docs, residue, pinned deps, small debt
