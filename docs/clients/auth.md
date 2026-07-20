---
title: Auth (sso-gateway)
description: AuthClient — ConnectRPC client for the sso-gateway IAM surface.
tags:
  - auth
  - sso-gateway
  - connectrpc
category: clients
nav_order: 20
created_at: "2026-07-20"
parent: ../getting-started.md
related:
  - ../testing.md
  - ../features.md
---

# Auth — sso-gateway IAM client

**Feature:** `auth` · **Module:** `sdk::auth`

`AuthClient` exposes the entire sso-gateway IAM surface as generated
ConnectRPC clients. The protobuf stubs are generated at build time from
`buf.build/sunbeamdotpt/sso-gateway` (codegen runs only when the `auth`
feature is enabled) and re-exported under `sdk::auth::v1`.

## Construction

```rust,no_run
use sdk::auth::AuthClient;

let client = AuthClient::builder("https://iam.example.com")
    .build()
    .expect("valid gateway URL");
```

`AuthClient` is cheap to clone (an `Arc` around the g2v HTTP stack). Attach a
tenant to every request with:

```rust,no_run
# let client = sdk::auth::AuthClient::builder("https://iam.example.com").build().unwrap();
let client = client.with_tenant("01HZY9JTKKHK3Y6XJJYHZ9Q5TV");
```

The tenant id travels in the `x-tenant-id` header
(`sdk::auth::TENANT_ID_HEADER`); per-call `CallOptions` headers override it.

## Service accessors

| Accessor | Service |
|---|---|
| `tenant()` | Tenant lifecycle |
| `identity()` | Identities |
| `identity_self_service()` | Self-service registration/recovery |
| `oauth2_device()` | OAuth2 device flow |
| `oauth2_consent()` | Consent management |
| `federation()` | OIDC discovery / federation |
| `permission()` | Relation tuples (OpenFGA/Keto) |
| `scim()` | SCIM provisioning |
| `application()` | OAuth applications |
| `client_credentials()` | Machine credentials |
| `agent()` | Agent service |

Each accessor returns a generated `*ServiceClient<ConnectTransport>`; call
methods with a request message and, for authenticated calls,
`*_with_options(req, CallOptions)` carrying a bearer token.

## Testing

`tests/sso_gateway_client.rs` boots the real stack via
`sdk::testing::SsoGateway` and exercises the generated clients end to end —
see [Testing](../testing.md#the-ssogateway-orchestrator). The suite is
serialized through a single-threaded nextest group because each test starts a
full five-container stack.
