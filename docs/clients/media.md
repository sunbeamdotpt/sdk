---
title: LiveKit Client
description: LiveKitClient — Twirp API for rooms, participants, and egress, plus JWT access tokens.
tags:
  - livekit
  - media
  - webrtc
category: clients
nav_order: 23
created_at: "2026-07-20"
parent: ../getting-started.md
related:
  - monitoring.md
  - ../testing.md
---

# LiveKit Client

**Feature:** `media` · **Module:** `sdk::media`

`LiveKitClient` speaks the LiveKit Twirp API (rooms, participants, egress)
and can mint the HMAC-SHA256 access tokens LiveKit expects.

## Construction

```rust,no_run
use sdk::media::{LiveKitClient, types::VideoGrants};
use sunbeam_g2v::client::{BearerToken, ClientBuilder};

// Server API calls need a token with admin-style video grants:
let token = LiveKitClient::generate_access_token(
    "api-key",
    "api-secret",
    "identity",
    &VideoGrants {
        room_create: Some(true),
        room_list: Some(true),
        room_admin: Some(true),
        ..Default::default()
    },
    600,
)?;

let g2v = ClientBuilder::new("https://livekit.example.com/")
    .auth(BearerToken::new(token))
    .build()
    .unwrap();
let client = LiveKitClient::new(&g2v);
# Ok::<(), sdk::error::SunbeamError>(())
```

## Rooms and participants

```rust,no_run
# use sdk::media::LiveKitClient;
# async fn example(client: LiveKitClient) -> sdk::error::Result<()> {
let room = client
    .create_room(&serde_json::json!({"name": "standup"}))
    .await?;
let rooms = client.list_rooms().await?;
client
    .delete_room(&serde_json::json!({"room": "standup"}))
    .await?;
# Ok(())
# }
```

- **Rooms** — `create_room`, `list_rooms`, `delete_room`,
  `update_room_metadata`, `send_data`
- **Participants** — `list_participants`, `get_participant`,
  `remove_participant`, `update_participant`, `mute_track`
- **Egress** — `start_room_composite_egress`, `start_track_egress`,
  `list_egress`, `stop_egress`

## Access tokens

`LiveKitClient::generate_access_token(api_key, api_secret, identity,
grants, ttl_secs)` signs a JWT with the `iss`/`sub`/`nbf`/`exp` claims and a
`video` grant object (`sdk::media::types::VideoGrants`: `room_create`,
`room_list`, `room_join`, `room_admin`, `can_publish`, …). No network call is
involved.

## Testing

`sdk::testing::LiveKit` boots `livekit/livekit-server` with the dev key pair
`devkey`/`devsecret`; `media::container_tests` runs a full room lifecycle
against it with a token signed by those keys.
