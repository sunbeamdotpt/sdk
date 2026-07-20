---
title: Matrix Client
description: MatrixClient — Matrix Client-Server API for rooms, messages, profiles, and more.
tags:
  - matrix
  - chat
category: clients
nav_order: 22
created_at: "2026-07-20"
parent: ../getting-started.md
related:
  - search.md
  - ../testing.md
---

# Matrix Client

**Feature:** `matrix` · **Module:** `sdk::matrix`

`MatrixClient` implements the Matrix Client-Server API (~80 endpoints) with
typed models in `sdk::matrix::types`. It targets the `/_matrix` base path —
remember the [trailing-slash rule](../getting-started.md#trailing-slash-rule).

## Construction

```rust,no_run
use sdk::matrix::MatrixClient;
use sunbeam_g2v::client::{BearerToken, ClientBuilder};

let g2v = ClientBuilder::new("https://messages.example.com/_matrix/")
    .auth(BearerToken::new("access-token"))
    .build()
    .unwrap();
let client = MatrixClient::new(&g2v);
```

`MatrixClient::connect(domain)` builds an unauthenticated client for
`https://messages.{domain}/_matrix/` — useful for `login` / `register`,
which return the access token you then attach to an authenticated client.

## Example

```rust,no_run
# use sdk::matrix::{MatrixClient, types::CreateRoomRequest};
# async fn example(client: MatrixClient) -> sdk::error::Result<()> {
let me = client.whoami().await?;

let room = client
    .create_room(&CreateRoomRequest {
        name: Some("standup".into()),
        ..Default::default()
    })
    .await?;

let event = client
    .send_event(
        &room.room_id,
        "m.room.message",
        "txn-1",
        &serde_json::json!({"msgtype": "m.text", "body": "hello"}),
    )
    .await?;
# Ok(())
# }
```

## API groups

- **Auth / account** — `login`, `refresh`, `logout`, `register`, `whoami`,
  password change, 3pids, deactivate
- **Rooms** — create, public room listing/search, directory visibility
- **Membership** — join/leave/invite/ban/unban/kick
- **State & messages** — state events, `sync`, `send_event`, `get_messages`,
  `get_event`, `get_context`, `redact`
- **Presence, typing, receipts**
- **Profiles** — display name, avatar
- **Aliases, user directory search**
- **Media** — `upload_media`, `download_media`, `thumbnail`
- **Devices, E2EE keys, push rules, notifications**
- **Account data, tags, message search, filters, spaces, send-to-device**

## Testing

`sdk::testing::Tuwunel` boots a Matrix homeserver with a known registration
token (`sunbeam-test-token`); `matrix::container_tests` registers a real user
and runs a room/message lifecycle.
