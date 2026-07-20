---
title: OpenSearch Client
description: OpenSearchClient — document, search, index, cluster, ingest, and snapshot APIs.
tags:
  - opensearch
  - search
category: clients
nav_order: 21
created_at: "2026-07-20"
parent: ../getting-started.md
related:
  - matrix.md
  - ../testing.md
---

# OpenSearch Client

**Feature:** `search` · **Module:** `sdk::search`

`OpenSearchClient` covers the OpenSearch HTTP API: documents, search,
indices, cluster, cat, ingest pipelines, and snapshots (~60 endpoints), with
typed request/response models in `sdk::search::types`.

## Construction

```rust,no_run
use sdk::search::OpenSearchClient;
use sunbeam_g2v::client::{BearerToken, ClientBuilder};

// Authenticated, shared g2v client:
let g2v = ClientBuilder::new("https://search.example.com/")
    .auth(BearerToken::new("token"))
    .build()
    .unwrap();
let client = OpenSearchClient::new(&g2v);

// Or unauthenticated against the standard hostname:
let client = OpenSearchClient::connect("example.com").unwrap();
```

Non-2xx responses become `SunbeamError::Network` with the response body
included.

## Examples

```rust,no_run
# use sdk::search::OpenSearchClient;
# async fn example(client: OpenSearchClient) -> sdk::error::Result<()> {
client
    .create_index("cards", &serde_json::json!({"settings": {"number_of_shards": 1}}))
    .await?;

client
    .index_doc("cards", "1", &serde_json::json!({"title": "hello world"}))
    .await?;

let found = client.index_exists("cards").await?;
let health = client.cluster_health().await?;

let res = client
    .search("cards", &serde_json::json!({"query": {"match": {"title": "hello"}}}))
    .await?;
println!("{} hits", res.hits.total.value);
# Ok(())
# }
```

## API groups

- **Documents** — `index_doc`, `get_doc`, `head_doc`, `update_doc`,
  `delete_doc`, `bulk`, `multi_get`, `reindex`, `delete_by_query`
- **Search** — `search`, `search_all`, `multi_search`, `count`, `scroll`,
  `clear_scroll`, `search_shards`, `search_template`
- **Indices** — `create_index`, `delete_index`, `get_index`, `index_exists`,
  settings/mapping/alias/template, `open_index`, `close_index`
- **Cluster** — `cluster_health`, `cluster_state`, `cluster_stats`,
  settings, `allocation_explain`, `reroute`
- **Nodes / cat** — `nodes_info`, `nodes_stats`, `nodes_hot_threads`,
  `cat_indices`, `cat_nodes`, `cat_shards`, `cat_health`, `cat_allocation`
- **Ingest / snapshots** — pipeline CRUD + simulate, snapshot repo and
  snapshot lifecycle

## Testing

`sdk::testing::OpenSearch` boots a single-node, security-disabled container;
`search::container_tests` shows a full document lifecycle against it.
