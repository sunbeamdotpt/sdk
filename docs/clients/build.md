---
title: Build Client
description: BuildKit image builds via the host buildctl binary.
tags:
  - buildkit
  - build
category: clients
nav_order: 25
created_at: "2026-07-20"
parent: ../getting-started.md
related:
  - ../features.md
---

# Build Client

**Feature:** `build` · **Module:** `sdk::build`

A thin wrapper around the host `buildctl` CLI for BuildKit image builds.
There is no daemon connection — `buildctl` must be installed and a BuildKit
daemon reachable (e.g. the cluster DaemonSet).

## Usage

```rust,ignore
use sdk::build::{BuildArgs, build, status};

let output = build(&BuildArgs {
    frontend: "dockerfile.v0".into(),
    local_context: ".".into(),
    local_dockerfile: ".".into(),
    output: "type=image,name=registry.example.com/app:latest,push=true".into(),
    build_args: vec![("VERSION".into(), "3.0.0".into())],
    ..Default::default()
})
.await?;

println!("digest: {}", output.digest);
```

- `build(&BuildArgs)` — run a build, returning `BuildOutput` (digest,
  success flag).
- `status()` — probe the BuildKit daemon.
- `prune()` — reclaim build cache.

Failures map to `SunbeamError::ExternalTool` (or `Build`) with the tool's
stderr attached. The module has no container test — it wraps a host binary;
`buildctl`-missing and argument-construction paths are covered by unit
tests.
