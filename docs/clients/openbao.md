---
title: OpenBao Client
description: BaoClient, secrets seeding, and the vault transit keystore.
tags:
  - openbao
  - vault
  - secrets
category: clients
nav_order: 28
created_at: "2026-07-20"
parent: ../getting-started.md
related:
  - ../testing.md
  - ../configuration.md
---

# OpenBao Client

**Features:** `openbao`, `secrets`, `vault-keystore` · **Modules:**
`sdk::openbao`, `sdk::secrets`, `sdk::vault_keystore`

## BaoClient

`BaoClient` wraps the OpenBao HTTP API (via `vaultrs`) with SDK error
mapping:

```rust,no_run
# use sdk::openbao::BaoClient;
# async fn example() -> sdk::error::Result<()> {
let bao = BaoClient::with_token("https://vault.example.com", "root-token");

let status = bao.seal_status().await?;
let mut data = std::collections::HashMap::new();
data.insert("key".to_string(), "value".to_string());
bao.kv_put("secret", "my-app", &data).await?;
let read = bao.kv_get("secret", "my-app").await?;
# Ok(())
# }
```

- **Lifecycle** — `seal_status`, `init`, `unseal`
- **KV v2** — `kv_get`, `kv_get_field`, `kv_put`, `kv_patch`, `kv_delete`
- **Engines & auth** — `enable_secrets_engine`, `auth_enable`, `write_policy`
- **Generic** — `read`, `list`, `write`
- **Database** — `write_db_config`, `write_db_static_role`

## secrets (feature)

Cluster-side operations: OpenBao init/unseal/seed, Vault Secrets Operator
secret sync, and port-forwarding to the in-cluster instance. Requires `kube`
+ `openbao`.

## vault_keystore (feature)

Local keystore files encrypted with age-style recipients and the Vault
transit engine (argon2 + aes-gcm). Used to keep short-lived root tokens off
disk in plaintext.

## Testing

`sdk::testing::OpenBao` boots a dev-mode container with the known root token
`root`; `openbao::container_tests` runs a KV round-trip against it.
