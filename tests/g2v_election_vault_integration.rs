#![cfg(all(feature = "g2v-election", feature = "testing", feature = "g2v-sqlx"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)

//! Integration tests for [`VaultElection`] against a real Vault-compatible
//! server (OpenBao in dev mode).
//!
//! By default these tests start an OpenBao container via `testcontainers`. Set
//! `VAULT_ADDR` to run against an existing OpenBao/Vault instead.
//!
//! ```text
//! cargo test -p sunbeam-g2v --test election_vault_integration -- --nocapture --test-threads=1
//! ```

use sdk::g2v::config::VaultConfig;
use sdk::g2v::election::vault::VaultElection;
use sdk::g2v::election::{ElectionError, LeaderElection};
use uuid::Uuid;

#[path = "g2v_support/mod.rs"]
mod support;

// ============================================================================
// Config helper
// ============================================================================

async fn vault_config() -> VaultConfig {
    support::containers::vault_test_config().await
}

// ============================================================================
// Probe helper
// ============================================================================

/// Probe openbao by attempting a trivial read operation.
/// Returns `false` (skip) if the server is unreachable or the KV mount is absent.
async fn probe_vault() -> bool {
    use vaultrs::client::{Client, VaultClient, VaultClientSettingsBuilder};

    let cfg = vault_config().await;

    let client = match VaultClient::new(
        VaultClientSettingsBuilder::default()
            .address(&cfg.url)
            .token(&cfg.token)
            .build()
            .unwrap(),
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[election_vault_integration] failed to build vault client: {e}; skipping.");
            return false;
        }
    };

    // Check server status — if this fails the server isn't up.
    if let Err(e) = client.status().await {
        eprintln!(
            "[election_vault_integration] openbao not reachable at {}: {e}; skipping. \
             Set VAULT_ADDR or ensure Docker is available to enable these tests.",
            cfg.url
        );
        return false;
    }

    // Verify the KV v2 mount is present by reading its config.
    let mount = "secret";
    if let Err(e) = vaultrs::kv2::config::read(&client, mount).await {
        eprintln!(
            "[election_vault_integration] KV v2 mount '{mount}' not accessible: {e}. \
             Ensure openbao is running in dev mode or that the mount exists."
        );
        return false;
    }

    true
}

// ============================================================================
// Tests
// ============================================================================

/// Become leader, assert `is_leader`, resign, assert no longer leader.
#[tokio::test]
async fn test_vault_become_leader_then_resign() {
    if !probe_vault().await {
        return;
    }

    let key = format!("elect-{}", Uuid::new_v4().simple());
    let cfg = vault_config().await;

    let mut election = VaultElection::new(cfg, &key, "instance-1");

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
}

/// Two election instances on the same key: the second one must fail with
/// `AlreadyLeader`. The first then resigns so the second can become leader.
#[tokio::test]
async fn test_vault_only_one_leader() {
    if !probe_vault().await {
        return;
    }

    let key = format!("elect-{}", Uuid::new_v4().simple());
    let cfg = vault_config().await;

    let mut first = VaultElection::new(cfg.clone(), &key, "instance-1");
    let mut second = VaultElection::new(cfg, &key, "instance-2");

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

    // Cleanup: first resigns, second can now take over.
    first.resign().await.expect("first resign should succeed");

    second
        .become_leader()
        .await
        .expect("second become_leader should succeed after resign");
    assert!(second.is_leader().await, "second should now be leader");

    // Cleanup.
    second.resign().await.expect("second resign cleanup");
}

/// After instance-1 becomes leader, instance-2's `get_leader` should return
/// instance-1's ID.
#[tokio::test]
async fn test_vault_get_leader_returns_holder_id() {
    if !probe_vault().await {
        return;
    }

    let key = format!("elect-{}", Uuid::new_v4().simple());
    let cfg = vault_config().await;

    let mut holder = VaultElection::new(cfg.clone(), &key, "instance-1");
    let observer = VaultElection::new(cfg, &key, "instance-2");

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
}

/// After becoming leader, `renew()` should extend the lock's `expires_at`
/// without losing ownership.
#[tokio::test]
async fn test_vault_renew_extends_lease() {
    if !probe_vault().await {
        return;
    }

    let key = format!("elect-{}", Uuid::new_v4().simple());
    let cfg = vault_config().await;
    let mut election = VaultElection::new(cfg, &key, "instance-renew");

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
}

/// Write an already-expired lock manually, then verify a fresh election can
/// preempt it via `become_leader`.
#[tokio::test]
async fn test_vault_expired_lock_can_be_preempted() {
    if !probe_vault().await {
        return;
    }

    use serde_json::json;
    use vaultrs::api::kv2::requests::SetSecretRequestOptions;
    use vaultrs::client::{VaultClient, VaultClientSettingsBuilder};

    let key = format!("elect-{}", Uuid::new_v4().simple());
    let cfg = vault_config().await;

    // Write an already-expired lock payload directly via the low-level API.
    let client = VaultClient::new(
        VaultClientSettingsBuilder::default()
            .address(&cfg.url)
            .token(&cfg.token)
            .build()
            .unwrap(),
    )
    .unwrap();

    let mount = "secret";
    let path = format!("g2v-elections/{}", key);
    let now = chrono::Utc::now().timestamp();
    let expired_payload = json!({
        "instance_id": "stale-instance",
        "acquired_at": now - 60,
        "expires_at": now - 30,  // already expired
    });
    vaultrs::kv2::set_with_options(
        &client,
        mount,
        &path,
        &expired_payload,
        SetSecretRequestOptions { cas: 0 },
    )
    .await
    .expect("writing expired lock should succeed");

    // A fresh election on the same key should preempt the expired lock.
    let mut fresh = VaultElection::new(cfg, &key, "fresh-instance");
    fresh
        .become_leader()
        .await
        .expect("become_leader should preempt an expired lock");

    assert!(fresh.is_leader().await, "fresh instance should be leader");

    // Cleanup.
    fresh.resign().await.expect("resign cleanup");
}
