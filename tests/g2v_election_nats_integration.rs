#![cfg(all(feature = "g2v-election", feature = "testing", feature = "g2v-sqlx"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)

//! Integration tests for [`NatsElection`] against a real NATS server.
//!
//! By default these tests start a NATS container via `testcontainers`. Set
//! `NATS_URL` to run against an existing NATS instead.
//!
//! ```text
//! cargo test -p sunbeam-g2v --test election_nats_integration -- --nocapture --test-threads=1
//! ```

use sdk::g2v::config::NatsConfig;
use sdk::g2v::election::nats::NatsElection;
use sdk::g2v::election::{ElectionError, LeaderElection};
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
// Cleanup helper
// ============================================================================

/// Delete a key from the `g2v-elections` bucket (best-effort; does not fail the
/// test if the key is already gone or the bucket doesn't exist yet).
async fn cleanup_key(key: &str) {
    let cfg = nats_config().await;
    let Ok(nats_client) = NatsClient::connect(&cfg).await else {
        return;
    };
    let client = nats_client.client().clone();
    let js = async_nats::jetstream::new(client);
    let Ok(kv) = js.get_key_value("g2v-elections").await else {
        return;
    };
    let _ = kv.delete(key).await;
}

// ============================================================================
// Tests
// ============================================================================

/// Become leader, assert `is_leader`, resign, assert no longer leader.
#[tokio::test]
async fn test_nats_become_leader_then_resign() {
    let key = format!("elect-{}", Uuid::new_v4().simple());
    cleanup_key(&key).await;

    let cfg = nats_config().await;
    let mut election = NatsElection::new(cfg, &key, "instance-1");

    assert!(
        !election.is_leader().await,
        "should not be leader before becoming one"
    );

    election
        .become_leader()
        .await
        .expect("become_leader should succeed");
    assert!(
        election.is_leader().await,
        "should be leader after become_leader"
    );

    election.resign().await.expect("resign should succeed");
    assert!(
        !election.is_leader().await,
        "should not be leader after resign"
    );

    cleanup_key(&key).await;
}

/// Two election instances on the same key: the second one must fail with
/// `AlreadyLeader`. First resigns so the second can take over.
#[tokio::test]
async fn test_nats_only_one_leader() {
    let key = format!("elect-{}", Uuid::new_v4().simple());
    cleanup_key(&key).await;

    let cfg = nats_config().await;
    let mut first = NatsElection::new(cfg.clone(), &key, "instance-1");
    let mut second = NatsElection::new(cfg, &key, "instance-2");

    first
        .become_leader()
        .await
        .expect("first become_leader should succeed");
    assert!(first.is_leader().await, "first should be leader");
    assert!(!second.is_leader().await, "second should not be leader");

    let result = second.become_leader().await;
    assert!(
        matches!(result, Err(ElectionError::AlreadyLeader)),
        "second become_leader must return AlreadyLeader, got: {result:?}"
    );

    // First resigns; second should now be able to take over.
    first.resign().await.expect("first resign should succeed");

    second
        .become_leader()
        .await
        .expect("second become_leader should succeed after resign");
    assert!(second.is_leader().await, "second should now be leader");

    // Cleanup.
    second.resign().await.expect("second resign cleanup");
    cleanup_key(&key).await;
}

/// After instance-1 becomes leader, instance-2's `get_leader` should return
/// instance-1's ID.
#[tokio::test]
async fn test_nats_get_leader_returns_holder_id() {
    let key = format!("elect-{}", Uuid::new_v4().simple());
    cleanup_key(&key).await;

    let cfg = nats_config().await;
    let mut holder = NatsElection::new(cfg.clone(), &key, "instance-1");
    let observer = NatsElection::new(cfg, &key, "instance-2");

    holder
        .become_leader()
        .await
        .expect("holder become_leader should succeed");

    let leader_id = observer.get_leader().await;
    assert_eq!(
        leader_id,
        Some("instance-1".to_string()),
        "observer should see instance-1 as leader"
    );

    // Cleanup.
    holder.resign().await.expect("holder resign cleanup");
    cleanup_key(&key).await;
}

/// After becoming leader, `renew()` should extend the lock's `expires_at`
/// without losing ownership.
#[tokio::test]
async fn test_nats_renew_extends_lease() {
    let key = format!("elect-{}", Uuid::new_v4().simple());
    cleanup_key(&key).await;

    let cfg = nats_config().await;
    let mut election = NatsElection::new(cfg, &key, "instance-renew");

    election
        .become_leader()
        .await
        .expect("become_leader should succeed");
    assert!(election.is_leader().await, "should be leader before renew");

    // Sleep 1 second so expires_at will visibly advance after renew.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    election.renew().await.expect("renew should succeed");
    assert!(
        election.is_leader().await,
        "should still be leader after renew"
    );

    // Cleanup.
    election.resign().await.expect("resign cleanup");
    cleanup_key(&key).await;
}

/// Write an already-expired lock payload manually, then verify a fresh
/// election can preempt it via `become_leader`.
#[tokio::test]
async fn test_nats_expired_lock_can_be_preempted() {
    let key = format!("elect-{}", Uuid::new_v4().simple());
    cleanup_key(&key).await;

    // Write an expired lock directly into the KV bucket.
    {
        let cfg = nats_config().await;
        let nats_client = NatsClient::connect(&cfg)
            .await
            .expect("connect for expired-lock write");
        let client = nats_client.client().clone();
        let js = async_nats::jetstream::new(client);
        let kv = match js
            .create_key_value(async_nats::jetstream::kv::Config {
                bucket: "g2v-elections".to_string(),
                description: "sunbeam-g2v leader election locks".to_string(),
                history: 1,
                ..Default::default()
            })
            .await
        {
            Ok(kv) => kv,
            Err(_) => js
                .get_key_value("g2v-elections")
                .await
                .expect("get bucket for expired-lock write"),
        };

        let now = chrono::Utc::now().timestamp();
        let expired = serde_json::json!({
            "instance_id": "stale-instance",
            "acquired_at": now - 60,
            "expires_at": now - 30,  // already expired
        });
        let bytes = serde_json::to_vec(&expired).unwrap();
        kv.put(&key, bytes::Bytes::from(bytes))
            .await
            .expect("write expired lock payload");
    }

    // A fresh election on the same key should preempt the expired lock.
    let cfg = nats_config().await;
    let mut fresh = NatsElection::new(cfg, &key, "fresh-instance");
    fresh
        .become_leader()
        .await
        .expect("become_leader should preempt an expired lock");

    assert!(fresh.is_leader().await, "fresh instance should be leader");

    // Cleanup.
    fresh.resign().await.expect("resign cleanup");
    cleanup_key(&key).await;
}
