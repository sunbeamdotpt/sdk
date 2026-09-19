//! Vault/OpenBAO-based leader election.
//!
//! # Strategy
//!
//! Uses the KV v2 secrets engine with compare-and-swap to implement a
//! distributed lock. The lock is a secret at `{config.kv_path}/{election_key}`
//! that contains `{ instance_id, acquired_at, expires_at }`.
//!
//! - `become_leader`: writes with `cas: 0` (create-only). If CAS fails,
//!   reads the current secret; if it is expired, deletes the metadata and
//!   retries once. Otherwise returns [`ElectionError::AlreadyLeader`](crate::g2v::election::ElectionError::AlreadyLeader).
//! - `is_leader`: reads the secret, compares `instance_id` to ours, and
//!   treats the lock as absent when `now() > expires_at`.
//! - `resign`: deletes all metadata for the secret (hard-delete so a
//!   subsequent `become_leader` with `cas: 0` succeeds again).
//! - `get_leader`: reads the `instance_id` from a non-expired lock secret.
//! - `renew`: re-writes the secret with a fresh `expires_at` using CAS on
//!   the current version so we fail fast if another instance has taken over.

#[cfg(feature = "g2v-vault")]
mod inner {
    use crate::g2v::config::VaultConfig;
    use crate::g2v::election::{ElectionError, ElectionResult, LeaderElection};

    use serde::{Deserialize, Serialize};
    use vaultrs::api::kv2::requests::SetSecretRequestOptions;
    use vaultrs::client::{VaultClient, VaultClientSettingsBuilder};
    use vaultrs::error::ClientError;

    // -------------------------------------------------------------------------
    // Payload stored at the lock path
    // -------------------------------------------------------------------------

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct LockPayload {
        instance_id: String,
        acquired_at: i64,
        /// Unix timestamp after which this lock is considered stale.
        expires_at: i64,
    }

    impl LockPayload {
        fn is_expired(&self) -> bool {
            chrono::Utc::now().timestamp() > self.expires_at
        }
    }

    // -------------------------------------------------------------------------
    // VaultElection
    // -------------------------------------------------------------------------

    /// Vault/OpenBAO-based leader election using KV v2 compare-and-swap.
    ///
    /// Construct via [`VaultElection::new`] and then drive the
    /// [`LeaderElection`] trait methods.
    #[derive(Debug, Clone)]
    pub struct VaultElection {
        config: VaultConfig,
        election_key: String,
        instance_id: String,
    }

    impl VaultElection {
        /// Create a new Vault-backed election instance.
        ///
        /// - `config` — Vault connection parameters (URL, token, kv_path,
        ///   lease_duration).
        /// - `election_key` — unique name for this election (appended to
        ///   `config.kv_path` as the secret path).
        /// - `instance_id` — stable identifier for this process/node.
        pub fn new(
            config: VaultConfig,
            election_key: impl Into<String>,
            instance_id: impl Into<String>,
        ) -> Self {
            Self {
                config,
                election_key: election_key.into(),
                instance_id: instance_id.into(),
            }
        }

        /// Build a fresh `VaultClient` from our config.
        ///
        /// We build per-call because `VaultClient` is not `Clone` and the
        /// trait methods take `&self` / `&mut self`.
        fn build_client(&self) -> ElectionResult<VaultClient> {
            VaultClient::new(
                VaultClientSettingsBuilder::default()
                    .address(&self.config.url)
                    .token(&self.config.token)
                    .build()
                    .map_err(|e| ElectionError::Failed(format!("vault client settings: {e}")))?,
            )
            .map_err(|e| ElectionError::Failed(format!("vault client build: {e}")))
        }

        /// Split `config.kv_path` into `(mount, prefix)`.
        ///
        /// The KV path convention used here is `<mount>/<prefix>`, e.g.
        /// `secret/sunbeam/elections`. The mount is the first path segment
        /// (`secret`) and the prefix is everything after (`sunbeam/elections`).
        fn mount_and_prefix(&self) -> (&str, &str) {
            let path = self.config.kv_path.trim_matches('/');
            match path.find('/') {
                Some(idx) => (&path[..idx], &path[idx + 1..]),
                None => (path, ""),
            }
        }

        /// Fully-qualified KV path for the election key.
        ///
        /// Returns `<prefix>/<election_key>` (or just `<election_key>` when
        /// `prefix` is empty).
        fn secret_path(&self) -> String {
            let (_, prefix) = self.mount_and_prefix();
            if prefix.is_empty() {
                self.election_key.clone()
            } else {
                format!("{}/{}", prefix, self.election_key)
            }
        }

        /// Build a fresh `LockPayload` for this instance with the configured TTL.
        fn fresh_payload(&self) -> LockPayload {
            let now = chrono::Utc::now().timestamp();
            LockPayload {
                instance_id: self.instance_id.clone(),
                acquired_at: now,
                expires_at: now + self.config.lease_duration as i64,
            }
        }

        /// Read the current lock secret, returning `None` when the secret does
        /// not exist (404 / `ResponseEmptyError`) or is expired.
        async fn read_lock(&self, client: &VaultClient) -> ElectionResult<Option<LockPayload>> {
            let (mount, _) = self.mount_and_prefix();
            let path = self.secret_path();

            match vaultrs::kv2::read::<LockPayload>(client, mount, &path).await {
                Ok(payload) if payload.is_expired() => Ok(None),
                Ok(payload) => Ok(Some(payload)),
                Err(ClientError::APIError { code: 404, .. }) => Ok(None),
                Err(ClientError::ResponseEmptyError) => Ok(None),
                Err(ClientError::ResponseDataEmptyError) => Ok(None),
                Err(e) => Err(ElectionError::Failed(format!("vault read: {e}"))),
            }
        }

        /// Read the raw lock without expiry filtering (needed for `renew` and
        /// expired-preemption paths that must see the physical payload).
        async fn read_lock_raw(&self, client: &VaultClient) -> ElectionResult<Option<LockPayload>> {
            let (mount, _) = self.mount_and_prefix();
            let path = self.secret_path();

            match vaultrs::kv2::read::<LockPayload>(client, mount, &path).await {
                Ok(payload) => Ok(Some(payload)),
                Err(ClientError::APIError { code: 404, .. }) => Ok(None),
                Err(ClientError::ResponseEmptyError) => Ok(None),
                Err(ClientError::ResponseDataEmptyError) => Ok(None),
                Err(e) => Err(ElectionError::Failed(format!("vault read raw: {e}"))),
            }
        }

        /// Hard-delete all metadata for the lock secret so a subsequent
        /// `become_leader` with `cas: 0` can succeed.
        async fn force_delete(&self, client: &VaultClient) -> ElectionResult<()> {
            let (mount, _) = self.mount_and_prefix();
            let path = self.secret_path();
            vaultrs::kv2::delete_metadata(client, mount, &path)
                .await
                .map_err(|e| ElectionError::Failed(format!("vault force delete: {e}")))
        }
    }

    #[async_trait::async_trait]
    impl LeaderElection for VaultElection {
        /// Attempt to become the leader.
        ///
        /// Writes with `cas: 0`. If that fails (CAS mismatch), reads the
        /// current secret; if it is expired, deletes the metadata and retries
        /// the write once. Otherwise returns [`ElectionError::AlreadyLeader`].
        async fn become_leader(&mut self) -> ElectionResult<()> {
            let client = self.build_client()?;
            let (mount, _) = self.mount_and_prefix();
            let path = self.secret_path();
            let payload = self.fresh_payload();
            let options = SetSecretRequestOptions { cas: 0 };

            match vaultrs::kv2::set_with_options(&client, mount, &path, &payload, options).await {
                Ok(_) => return Ok(()),
                Err(ClientError::APIError { code: 400, .. }) => {
                    // CAS mismatch — the secret physically exists.  Check whether
                    // it is expired so we can preempt a stale lock.
                }
                Err(e) => return Err(ElectionError::Failed(format!("vault become_leader: {e}"))),
            }

            // CAS failed — inspect the existing secret.
            match self.read_lock_raw(&client).await? {
                Some(existing) if existing.is_expired() => {
                    // Expired — delete and retry once.
                    self.force_delete(&client).await?;
                    let retry_options = SetSecretRequestOptions { cas: 0 };
                    vaultrs::kv2::set_with_options(
                        &client,
                        mount,
                        &path,
                        &self.fresh_payload(),
                        retry_options,
                    )
                    .await
                    .map(|_| ())
                    .map_err(|e| ElectionError::Failed(format!("vault become_leader retry: {e}")))
                }
                _ => Err(ElectionError::AlreadyLeader),
            }
        }

        /// Returns `true` when the current lock secret carries our instance ID
        /// and has not expired.
        async fn is_leader(&self) -> bool {
            let Ok(client) = self.build_client() else {
                return false;
            };
            match self.read_lock(&client).await {
                Ok(Some(payload)) => payload.instance_id == self.instance_id,
                _ => false,
            }
        }

        /// Resign by hard-deleting the lock secret.
        ///
        /// Uses `delete_metadata` which removes all versions + metadata so a
        /// fresh `become_leader` with `cas: 0` can succeed afterwards.
        ///
        /// Returns [`ElectionError::NotLeader`] when we do not currently hold
        /// the lock.
        async fn resign(&mut self) -> ElectionResult<()> {
            let client = self.build_client()?;

            // Verify we hold the (non-expired) lock before deleting.
            match self.read_lock(&client).await? {
                Some(payload) if payload.instance_id == self.instance_id => {}
                Some(_) => return Err(ElectionError::NotLeader),
                None => return Err(ElectionError::NotLeader),
            }

            self.force_delete(&client).await
        }

        /// Return the instance ID of the current, non-expired lock holder, or `None`.
        async fn get_leader(&self) -> Option<String> {
            let client = self.build_client().ok()?;
            self.read_lock(&client)
                .await
                .ok()
                .flatten()
                .map(|p| p.instance_id)
        }

        /// Re-write the lock with a fresh `expires_at` using CAS on the current
        /// version.
        ///
        /// Returns [`ElectionError::NotLeader`] if we no longer own the lock or
        /// if the CAS version check fails (another instance has taken over).
        async fn renew(&mut self) -> ElectionResult<()> {
            let client = self.build_client()?;
            let (mount, _) = self.mount_and_prefix();
            let path = self.secret_path();

            // Read raw (not expiry-filtered) to get the current version number
            // via a subsequent metadata read, and to verify ownership.
            let existing = self
                .read_lock_raw(&client)
                .await?
                .ok_or(ElectionError::NotLeader)?;

            if existing.instance_id != self.instance_id {
                return Err(ElectionError::NotLeader);
            }

            // Fetch the current metadata to get the version for CAS.
            let meta = vaultrs::kv2::read_metadata(&client, mount, &path)
                .await
                .map_err(|e| ElectionError::Failed(format!("vault renew read_metadata: {e}")))?;

            let current_version = meta.current_version;

            let renewed = LockPayload {
                instance_id: self.instance_id.clone(),
                acquired_at: existing.acquired_at,
                expires_at: chrono::Utc::now().timestamp() + self.config.lease_duration as i64,
            };

            let options = SetSecretRequestOptions {
                cas: current_version as u32,
            };

            match vaultrs::kv2::set_with_options(&client, mount, &path, &renewed, options).await {
                Ok(_) => Ok(()),
                Err(ClientError::APIError { code: 400, .. }) => Err(ElectionError::NotLeader),
                Err(e) => Err(ElectionError::Failed(format!("vault renew write: {e}"))),
            }
        }
    }

    // -------------------------------------------------------------------------
    // Unit tests (no live Vault required)
    // -------------------------------------------------------------------------

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_vault_election_new() {
            let config = VaultConfig::default();
            let election = VaultElection::new(config, "my_election", "instance-1");
            assert_eq!(election.election_key, "my_election");
            assert_eq!(election.instance_id, "instance-1");
        }

        #[test]
        fn test_mount_and_prefix_with_slash() {
            let config = VaultConfig {
                kv_path: "secret/sunbeam/elections".to_string(),
                ..Default::default()
            };
            let election = VaultElection::new(config, "key", "id");
            let (mount, prefix) = election.mount_and_prefix();
            assert_eq!(mount, "secret");
            assert_eq!(prefix, "sunbeam/elections");
        }

        #[test]
        fn test_mount_and_prefix_no_slash() {
            let config = VaultConfig {
                kv_path: "secret".to_string(),
                ..Default::default()
            };
            let election = VaultElection::new(config, "key", "id");
            let (mount, prefix) = election.mount_and_prefix();
            assert_eq!(mount, "secret");
            assert_eq!(prefix, "");
        }

        #[test]
        fn test_secret_path_with_prefix() {
            let config = VaultConfig {
                kv_path: "secret/elections".to_string(),
                ..Default::default()
            };
            let election = VaultElection::new(config, "my-election", "id");
            assert_eq!(election.secret_path(), "elections/my-election");
        }

        #[test]
        fn test_secret_path_no_prefix() {
            let config = VaultConfig {
                kv_path: "secret".to_string(),
                ..Default::default()
            };
            let election = VaultElection::new(config, "my-election", "id");
            assert_eq!(election.secret_path(), "my-election");
        }

        #[test]
        fn test_lock_payload_not_expired() {
            let payload = LockPayload {
                instance_id: "inst-1".to_string(),
                acquired_at: chrono::Utc::now().timestamp(),
                expires_at: chrono::Utc::now().timestamp() + 60,
            };
            assert!(!payload.is_expired());
        }

        #[test]
        fn test_lock_payload_expired() {
            let payload = LockPayload {
                instance_id: "inst-1".to_string(),
                acquired_at: 1_000_000,
                expires_at: 1_000_030,
            };
            assert!(payload.is_expired());
        }

        #[test]
        fn test_fresh_payload_has_ttl() {
            let config = VaultConfig {
                lease_duration: 45,
                ..VaultConfig::default()
            };
            let election = VaultElection::new(config, "key", "inst-1");
            let p = election.fresh_payload();
            assert_eq!(p.instance_id, "inst-1");
            let ttl = p.expires_at - p.acquired_at;
            assert_eq!(ttl, 45);
        }

        #[test]
        fn test_lock_payload_roundtrip() {
            let payload = LockPayload {
                instance_id: "node-7".to_string(),
                acquired_at: 1_700_000_000,
                expires_at: 1_700_000_030,
            };
            let json = serde_json::to_string(&payload).unwrap();
            let decoded: LockPayload = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded.instance_id, "node-7");
            assert_eq!(decoded.acquired_at, 1_700_000_000);
            assert_eq!(decoded.expires_at, 1_700_000_030);
        }
    }
}

#[cfg(feature = "g2v-vault")]
pub use inner::VaultElection;
