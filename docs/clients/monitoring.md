---
title: Monitoring Clients
description: PrometheusClient, LokiClient, and GrafanaClient for the observability stack.
tags:
  - prometheus
  - loki
  - grafana
  - monitoring
category: clients
nav_order: 24
created_at: "2026-07-20"
parent: ../getting-started.md
related:
  - media.md
  - ../testing.md
---

# Monitoring Clients

**Feature:** `monitoring` · **Module:** `sdk::monitoring`

Three clients for the observability stack, re-exported from
`sdk::monitoring` with shared response types in `sdk::monitoring::types`
(both Prometheus and Loki answer `{"status":"success","data":{…}}`).

## Prometheus

`PrometheusClient` targets the `/api/v1` base path
(`https://systemmetrics.{domain}/api/v1/` via `connect`).

- **Query** — `query`, `query_range`, `format_query`
- **Metadata** — `series`, `labels`, `label_values`, `targets_metadata`,
  `metadata`
- **Infrastructure** — `targets`, `scrape_pools`, `alertmanagers`, `rules`,
  `alerts`
- **Status** — `config`, `flags`, `runtime_info`, `build_info`, `tsdb`

## Loki

`LokiClient` targets `/loki/api/v1`
(`https://systemlogs.{domain}/loki/api/v1/` via `connect`).

- **Query** — `query`, `query_range`, `labels`, `label_values`, `series`
- **Index** — `index_stats`, `index_volume`, `index_volume_range`
- **Patterns** — `detect_patterns`
- **Ingest** — `push`
- **Status** — `ready` (hits the server-root `/ready`, bypassing the API
  prefix automatically)

> Plain selector queries (`{job="x"}`) are only valid as **range** queries in
> current Loki — use `query_range`, not `query`, for log lines.

## Grafana

`GrafanaClient` targets `/api` (`https://metrics.{domain}/api/` via
`connect`). Grafana supports basic auth — configure it as a default header:

```rust,no_run
# use sunbeam_g2v::client::ClientBuilder;
# use base64::Engine;
let basic = base64::engine::general_purpose::STANDARD.encode("admin:admin");
let g2v = ClientBuilder::new("https://grafana.example.com/api/")
    .default_header("Authorization", format!("Basic {basic}"))
    .unwrap()
    .build()
    .unwrap();
# let client = sdk::monitoring::GrafanaClient::new(&g2v);
```

- **Dashboards** — create/get/update/delete, list, search
- **Datasources** — list/get/create/update/delete, `proxy_datasource`;
  **UID variants** (`get_datasource_by_uid`, `delete_datasource_by_uid`) —
  numeric-ID endpoints are removed in Grafana ≥ 11, prefer the UID ones
- **Folders, annotations, provisioned alert rules, org**

## Testing

`sdk::testing::{Prometheus, Loki, Grafana}` boot stock containers;
`monitoring::container_tests` covers instant queries, log push + range
query, and a datasource lifecycle.
