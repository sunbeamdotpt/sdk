---
title: WFE Client
description: wfectl — remote workflow engine control via gRPC.
tags:
  - wfe
  - workflows
  - grpc
category: clients
nav_order: 27
created_at: "2026-07-20"
parent: ../getting-started.md
related:
  - build.md
  - ../features.md
---

# WFE Client

**Feature:** `wfectl` · **Module:** `sdk::wfectl`

gRPC client for the WFE workflow engine, built on tonic with the
`wfe-server-protos` definitions.

- **List / inspect** — enumerate workflows and runs
- **Run** — trigger a workflow with parameters
- **Logs** — stream or fetch run logs
- **Control** — `cancel`, `suspend`, `resume`
- **Publish** — push workflow definitions remotely

The client resolves its bearer token from the environment
(`WFE_TOKEN`-style resolution — see `sdk::wfectl::resolve_token`) and
validates it before injecting the `Authorization` header.
