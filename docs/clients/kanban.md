---
title: Kanban Client
description: Kanban boards, cards, projects, templates, attachments, and search via ConnectRPC.
tags:
  - kanban
  - connectrpc
category: clients
nav_order: 26
created_at: "2026-07-20"
parent: ../getting-started.md
related:
  - auth.md
  - ../features.md
---

# Kanban Client

**Feature:** `kanban` · **Module:** `sdk::kanban`

ConnectRPC client for the Sunbeam Kanban service: boards, cards, projects,
templates, attachments, full-text search, and real-time subscriptions. The
stubs are generated at build time from `buf.build/sunbeamdotpt/kanban`
(codegen runs only with the `kanban` feature) and re-exported under
`sdk::kanban::v1`.

- **Boards** — CRUD, aggregated board views, public boards
- **Cards** — CRUD, movement, comments, attachments (upload/download)
- **Projects & templates** — scaffolding new boards
- **Search** — full-text card search backed by the server's OpenSearch index
- **Events** — subscription stream for live updates

The service layer is tested behind `mockall::automock` traits (the module
targets >90% line coverage via `cargo llvm-cov`), so consumers can mock the
client surface in their own tests the same way.
