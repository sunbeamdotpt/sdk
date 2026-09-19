#![cfg(all(feature = "g2v-nats", feature = "testing", feature = "g2v-sqlx"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)

//! Integration tests for [`NatsClient`] against a real NATS server.
//!
//! By default these tests start a NATS container via `testcontainers`. Set
//! `NATS_URL` to run against an existing NATS instead.
//!
//! ```text
//! cargo test -p sunbeam-g2v --test nats_integration -- --nocapture --test-threads=1
//! ```

use futures::StreamExt;
use sdk::g2v::config::NatsConfig;
use sdk::g2v::mq::NatsClient;
use uuid::Uuid;

#[path = "g2v_support/mod.rs"]
mod support;

// ============================================================================
// Config helper
// ============================================================================

async fn nats_config() -> NatsConfig {
    let url = support::containers::nats_url().await;
    NatsConfig {
        url,
        jetstream: true,
        lease_duration: 30,
        auth_token: None,
    }
}

// ============================================================================
// Tests
// ============================================================================

/// Connect to the NATS container and assert `is_connected()`.
#[tokio::test]
async fn test_connect_to_workspace_nats() {
    let cfg = nats_config().await;
    let client = NatsClient::connect(&cfg)
        .await
        .expect("connect should succeed");
    assert!(
        client.is_connected(),
        "client should report Connected after successful connect"
    );
}

/// Publish a message, subscribe to the same subject, assert the payload arrives.
///
/// Uses a UUID-suffixed subject to avoid collisions with concurrent runs.
#[tokio::test]
async fn test_publish_subscribe_round_trip() {
    let cfg = nats_config().await;
    let client = NatsClient::connect(&cfg)
        .await
        .expect("connect should succeed");

    let subject = format!("g2v.test.{}", Uuid::new_v4());
    let payload = bytes::Bytes::from("hello from sunbeam-g2v integration test");

    let mut sub = client
        .subscribe(&subject)
        .await
        .expect("subscribe should succeed");

    client
        .publish(&subject, payload.clone())
        .await
        .expect("publish should succeed");

    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), sub.next())
        .await
        .expect("timed out waiting for message")
        .expect("subscriber closed before message arrived");

    assert_eq!(
        msg.payload, payload,
        "received payload must match sent payload"
    );
}

/// Set up a responder task that subscribes and sends a reply; assert `request()`
/// returns the reply.
#[tokio::test]
async fn test_request_reply() {
    let cfg = nats_config().await;
    let client = NatsClient::connect(&cfg)
        .await
        .expect("connect should succeed");

    let subject = format!("g2v.test.req.{}", Uuid::new_v4());
    let reply_payload = bytes::Bytes::from("pong");

    // Spawn a responder that listens and echoes back "pong".
    let responder_client = client.clone();
    let responder_subject = subject.clone();
    let responder_reply = reply_payload.clone();
    let handle = tokio::spawn(async move {
        let mut sub = responder_client
            .subscribe(&responder_subject)
            .await
            .expect("responder subscribe");

        if let Some(msg) = sub.next().await
            && let Some(reply) = msg.reply
        {
            responder_client
                .client()
                .publish(reply, responder_reply)
                .await
                .expect("responder publish reply");
        }
    });

    // Small yield to let the responder subscribe before we send.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.request(&subject, bytes::Bytes::from("ping")),
    )
    .await
    .expect("timed out waiting for reply")
    .expect("request should succeed");

    assert_eq!(response.payload, reply_payload);

    let _ = handle.await;
}

/// Call `ensure_stream` twice for the same stream config (UUID-suffixed name),
/// assert no error on either call (idempotent). Cleanup: delete the stream after.
#[tokio::test]
async fn test_jetstream_ensure_stream_idempotent() {
    let cfg = nats_config().await;
    let client = NatsClient::connect(&cfg)
        .await
        .expect("connect should succeed");

    let stream_name = format!("G2VTEST{}", Uuid::new_v4().simple());

    let config = async_nats::jetstream::stream::Config {
        name: stream_name.clone(),
        subjects: vec![format!("g2v.test.stream.{stream_name}.>")],
        ..Default::default()
    };

    // First call — creates the stream.
    client
        .ensure_stream(config.clone())
        .await
        .expect("first ensure_stream should succeed");

    // Second call — should be a no-op / idempotent.
    client
        .ensure_stream(config.clone())
        .await
        .expect("second ensure_stream should be idempotent");

    // Cleanup.
    let js = client.jetstream().expect("jetstream must be Some");
    js.delete_stream(&stream_name)
        .await
        .expect("cleanup: delete_stream should succeed");
}

/// Create a JetStream stream, publish via JetStream, pull-consume, assert the
/// message arrives. Cleanup the stream after.
#[tokio::test]
async fn test_jetstream_publish_and_consume() {
    let cfg = nats_config().await;
    let client = NatsClient::connect(&cfg)
        .await
        .expect("connect should succeed");

    let stream_name = format!("G2VPUB{}", Uuid::new_v4().simple());
    let subject = format!("g2v.test.js.{stream_name}");
    let payload = bytes::Bytes::from("jetstream test message");

    // Create the stream.
    let config = async_nats::jetstream::stream::Config {
        name: stream_name.clone(),
        subjects: vec![subject.clone()],
        ..Default::default()
    };
    client
        .ensure_stream(config)
        .await
        .expect("ensure_stream should succeed");

    // Publish via JetStream and await the ack.
    let ack_future = client
        .publish_jetstream(&subject, payload.clone())
        .await
        .expect("publish_jetstream should succeed");
    ack_future
        .await
        .expect("JetStream publish ack should succeed");

    // Pull-subscribe: create an ephemeral consumer with a UUID name.
    let consumer_name = format!("g2v-test-{}", Uuid::new_v4().simple());
    let consumer = client
        .pull_subscribe(
            &stream_name,
            async_nats::jetstream::consumer::pull::Config {
                name: Some(consumer_name.clone()),
                durable_name: Some(consumer_name.clone()),
                ..Default::default()
            },
        )
        .await
        .expect("pull_subscribe should succeed");

    // Fetch one message with a timeout.
    let mut messages = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        consumer.fetch().max_messages(1).messages(),
    )
    .await
    .expect("timed out fetching messages")
    .expect("fetch should return a message stream");

    use futures::StreamExt;
    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), messages.next())
        .await
        .expect("timed out waiting for pulled message")
        .expect("message stream ended before message arrived")
        .expect("message error");

    assert_eq!(
        msg.payload, payload,
        "pulled message payload must match published payload"
    );
    msg.ack().await.expect("ack should succeed");

    // Cleanup.
    let js = client.jetstream().expect("jetstream must be Some");
    js.delete_stream(&stream_name)
        .await
        .expect("cleanup: delete_stream should succeed");
}
