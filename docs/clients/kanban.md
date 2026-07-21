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

## Building a client

`KanbanClient::connect(url)` builds an unauthenticated client in one call;
use `KanbanClient::builder(url)` when you need auth or other g2v options:

```rust,no_run
use sdk::kanban::KanbanClient;
use sdk::kanban::prelude::sunbeam_g2v::client::BearerToken;

# fn example() {
// Unauthenticated, one call:
let client = KanbanClient::connect("https://kanban.example.com").unwrap();

// Authenticated:
let g2v = KanbanClient::builder("https://kanban.example.com")
    .auth(BearerToken::new("my-token"))
    .build()
    .unwrap();
let client = KanbanClient::new(g2v, "https://kanban.example.com".parse().unwrap()).unwrap();

// Default header on every request (per-call CallOptions headers win):
let client = client.with_default_header("x-sunbeam-object-id", "board-123");
# }
```

The generated API surface exposes types from `connectrpc`, `buffa`,
`buffa-types`, and `sunbeam-g2v` (`CallOptions`, `ConnectError`,
`MessageField`, `FieldMask`, `Timestamp`, `BearerToken`, …). Import them via
`sdk::kanban::prelude` instead of adding direct dependencies so the versions
always match the SDK's:

```rust,no_run
use sdk::kanban::prelude::connectrpc::client::CallOptions;
```

`SunbeamError` converts from `connectrpc::ConnectError` (feature `kanban` or
`auth`), so RPC results plug into the SDK error tree with `?`.

For end-to-end tests against a real server, see the
[`Kanban` testcontainers orchestrator](../testing.md#the-kanban-orchestrator).
