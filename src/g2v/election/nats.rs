//! NATS JetStream KV-based leader election.
//!
//! # Strategy
//!
//! Uses NATS JetStream's key-value store with the `create` (put-if-not-exists)
//! primitive as a distributed lock.
//!
//! - KV bucket: `g2v-elections` (created on first use via `get_or_create_key_value`).
//! - Lock key: `{election_key}`.
//! - Lock value: `{ instance_id, acquired_at }` JSON bytes.
//!
//! - `become_leader`: calls `kv.create(key, payload)`. Succeeds when the key
//!   does not yet exist; returns [`ElectionError::AlreadyLeader`](crate::g2v::election::ElectionError::AlreadyLeader) on
//!   `CreateErrorKind::AlreadyExists`.
//! - `is_leader`: reads the current value and compares `instance_id` to ours.
//! - `resign`: deletes the key. Checks ownership first.
//! - `get_leader`: reads the current value and returns `instance_id`.
//!
//! # Limitations (v1)
//!
//! The lock has no automatic expiry. If the holding process dies without
//! calling `resign`, the key remains set. A future version should configure a
//! per-message TTL on the bucket so stale locks expire automatically.

#[cfg(feature = "g2v-nats")]
mod inner {
    use crate::g2v::config::NatsConfig;
    use crate::g2v::election::{ElectionError, ElectionResult, LeaderElection};
    use crate::g2v::nats_util::parse_nats_url;

    use async_nats::jetstream::kv::{Config as KvConfig, CreateErrorKind, Store};
    use serde::{Deserialize, Serialize};

    /// Bucket name shared by all `g2v` election instances.
    const ELECTION_BUCKET: &str = "g2v-elections";

    // -------------------------------------------------------------------------
    // Payload stored under the election key
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
    // NatsElection
    // -------------------------------------------------------------------------

    /// NATS JetStream KV-backed leader election.
    ///
    /// Construct via [`NatsElection::new`] and then drive the
    /// [`LeaderElection`] trait methods.
    #[derive(Debug, Clone)]
    pub struct NatsElection {
        config: NatsConfig,
        election_key: String,
        instance_id: String,
    }

    impl NatsElection {
        /// Create a new NATS-backed election instance.
        ///
        /// - `config` — NATS connection parameters (url, jetstream,
        ///   lease_duration).
        /// - `election_key` — unique name for this election (used as the KV key).
        /// - `instance_id` — stable identifier for this process/node.
        pub fn new(
            config: NatsConfig,
            election_key: impl Into<String>,
            instance_id: impl Into<String>,
        ) -> Self {
            Self {
                config,
                election_key: election_key.into(),
                instance_id: instance_id.into(),
            }
        }

        /// Connect to NATS and return the election KV bucket.
        ///
        /// The bucket is created via `create_key_value` if it does not exist
        /// yet, using `get_key_value` as a fallback so the call is idempotent.
        async fn bucket(&self) -> ElectionResult<Store> {
            let (url, url_token) = parse_nats_url(&self.config.url);
            let token = self.config.auth_token.clone().or(url_token);

            let mut opts = async_nats::ConnectOptions::new();
            if let Some(t) = token {
                opts = opts.token(t);
            }

            let client = opts
                .connect(&url)
                .await
                .map_err(|e| ElectionError::Failed(format!("nats connect: {e}")))?;

            let js = async_nats::jetstream::new(client);

            // Try to create the bucket; if it already exists fall back to get.
            let store = match js
                .create_key_value(KvConfig {
                    bucket: ELECTION_BUCKET.to_string(),
                    description: "sunbeam-g2v leader election locks".to_string(),
                    history: 1,
                    ..Default::default()
                })
                .await
            {
                Ok(store) => store,
                Err(_) => js
                    .get_key_value(ELECTION_BUCKET)
                    .await
                    .map_err(|e| ElectionError::Failed(format!("nats kv get bucket: {e}")))?,
            };

            Ok(store)
        }

        /// Encode a fresh `LockPayload` for this instance.
        fn encode_fresh_payload(&self) -> ElectionResult<bytes::Bytes> {
            let now = chrono::Utc::now().timestamp();
            let payload = LockPayload {
                instance_id: self.instance_id.clone(),
                acquired_at: now,
                expires_at: now + self.config.lease_duration as i64,
            };
            let json = serde_json::to_vec(&payload)
                .map_err(|e| ElectionError::Failed(format!("nats payload encode: {e}")))?;
            Ok(bytes::Bytes::from(json))
        }

        /// Decode a `LockPayload` from raw bytes, returning `None` on parse error.
        fn decode_payload(bytes: bytes::Bytes) -> Option<LockPayload> {
            serde_json::from_slice(&bytes).ok()
        }
    }

    #[async_trait::async_trait]
    impl LeaderElection for NatsElection {
        /// Attempt to become the leader.
        ///
        /// Uses `kv.create` (put-if-not-exists). If the key already exists but
        /// the stored payload is expired, deletes it and retries once.
        /// Otherwise returns [`ElectionError::AlreadyLeader`].
        async fn become_leader(&mut self) -> ElectionResult<()> {
            let kv = self.bucket().await?;
            let payload = self.encode_fresh_payload()?;

            match kv.create(&self.election_key, payload).await {
                Ok(_) => return Ok(()),
                Err(e) if e.kind() == CreateErrorKind::AlreadyExists => {
                    // Key exists — check whether the lock is expired.
                }
                Err(e) => return Err(ElectionError::Failed(format!("nats become_leader: {e}"))),
            }

            // Read the existing payload and check expiry.
            match kv.get(&self.election_key).await {
                Ok(Some(bytes)) => {
                    let existing = Self::decode_payload(bytes).ok_or_else(|| {
                        ElectionError::Failed("nats: corrupt lock payload".into())
                    })?;

                    if !existing.is_expired() {
                        return Err(ElectionError::AlreadyLeader);
                    }

                    // Expired — delete and retry create once.
                    kv.delete(&self.election_key).await.map_err(|e| {
                        ElectionError::Failed(format!("nats preempt expired delete: {e}"))
                    })?;

                    let retry_payload = self.encode_fresh_payload()?;
                    kv.create(&self.election_key, retry_payload)
                        .await
                        .map(|_| ())
                        .map_err(|e| {
                            ElectionError::Failed(format!("nats become_leader retry: {e}"))
                        })
                }
                Ok(None) => {
                    // Key was deleted between our create attempt and now;
                    // retry once.
                    let retry_payload = self.encode_fresh_payload()?;
                    kv.create(&self.election_key, retry_payload)
                        .await
                        .map(|_| ())
                        .map_err(|e| {
                            ElectionError::Failed(format!("nats become_leader retry: {e}"))
                        })
                }
                Err(e) => Err(ElectionError::Failed(format!(
                    "nats become_leader read: {e}"
                ))),
            }
        }

        /// Returns `true` when the current lock key carries our instance ID and
        /// has not expired.
        async fn is_leader(&self) -> bool {
            let Ok(kv) = self.bucket().await else {
                return false;
            };
            match kv.get(&self.election_key).await {
                Ok(Some(bytes)) => Self::decode_payload(bytes)
                    .map(|p| p.instance_id == self.instance_id && !p.is_expired())
                    .unwrap_or(false),
                _ => false,
            }
        }

        /// Resign by deleting the lock key.
        ///
        /// Returns [`ElectionError::NotLeader`] when we do not currently hold
        /// the (non-expired) lock.
        async fn resign(&mut self) -> ElectionResult<()> {
            let kv = self.bucket().await?;

            // Verify ownership before deleting.
            match kv.get(&self.election_key).await {
                Ok(Some(bytes)) => {
                    let payload = Self::decode_payload(bytes).ok_or_else(|| {
                        ElectionError::Failed("nats: corrupt lock payload".into())
                    })?;
                    if payload.instance_id != self.instance_id || payload.is_expired() {
                        return Err(ElectionError::NotLeader);
                    }
                }
                Ok(None) => return Err(ElectionError::NotLeader),
                Err(e) => return Err(ElectionError::Failed(format!("nats resign read: {e}"))),
            }

            kv.delete(&self.election_key)
                .await
                .map_err(|e| ElectionError::Failed(format!("nats resign delete: {e}")))?;

            Ok(())
        }

        /// Return the instance ID of the current, non-expired lock holder, or `None`.
        async fn get_leader(&self) -> Option<String> {
            let kv = self.bucket().await.ok()?;
            let bytes = kv.get(&self.election_key).await.ok()??;
            Self::decode_payload(bytes)
                .filter(|p| !p.is_expired())
                .map(|p| p.instance_id)
        }

        /// Re-write the lock with a fresh `expires_at` by overwriting with `kv.put`.
        ///
        /// Reads the current payload first to verify ownership. Returns
        /// [`ElectionError::NotLeader`] if we don't own the lock or it has
        /// already expired.
        async fn renew(&mut self) -> ElectionResult<()> {
            let kv = self.bucket().await?;

            // Read current value to verify we still own it.
            let existing = match kv.get(&self.election_key).await {
                Ok(Some(bytes)) => Self::decode_payload(bytes)
                    .ok_or_else(|| ElectionError::Failed("nats renew: corrupt payload".into()))?,
                Ok(None) => return Err(ElectionError::NotLeader),
                Err(e) => return Err(ElectionError::Failed(format!("nats renew read: {e}"))),
            };

            if existing.instance_id != self.instance_id || existing.is_expired() {
                return Err(ElectionError::NotLeader);
            }

            let renewed = LockPayload {
                instance_id: self.instance_id.clone(),
                acquired_at: existing.acquired_at,
                expires_at: chrono::Utc::now().timestamp() + self.config.lease_duration as i64,
            };
            let json = serde_json::to_vec(&renewed)
                .map_err(|e| ElectionError::Failed(format!("nats renew encode: {e}")))?;

            kv.put(&self.election_key, bytes::Bytes::from(json))
                .await
                .map(|_| ())
                .map_err(|e| ElectionError::Failed(format!("nats renew put: {e}")))
        }
    }

    // -------------------------------------------------------------------------
    // Unit tests (no live NATS required)
    // -------------------------------------------------------------------------

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_nats_election_new() {
            let config = NatsConfig::default();
            let election = NatsElection::new(config, "my_election", "instance-1");
            assert_eq!(election.election_key, "my_election");
            assert_eq!(election.instance_id, "instance-1");
        }

        #[test]
        fn test_decode_payload_roundtrip() {
            let payload = LockPayload {
                instance_id: "node-42".to_string(),
                acquired_at: 1_700_000_000,
                expires_at: 1_700_000_030,
            };
            let encoded = serde_json::to_vec(&payload).unwrap();
            let decoded = NatsElection::decode_payload(bytes::Bytes::from(encoded)).unwrap();
            assert_eq!(decoded.instance_id, "node-42");
            assert_eq!(decoded.acquired_at, 1_700_000_000);
            assert_eq!(decoded.expires_at, 1_700_000_030);
        }

        #[test]
        fn test_decode_payload_bad_bytes() {
            let result = NatsElection::decode_payload(bytes::Bytes::from(b"not-json".as_slice()));
            assert!(result.is_none());
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
    }
}

#[cfg(feature = "g2v-nats")]
pub use inner::NatsElection;
