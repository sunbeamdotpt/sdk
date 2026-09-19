//! Leader election for Sunbeam services.

pub mod memory;
pub mod nats;
pub mod vault;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// Error type for leader election.
#[derive(Debug, Clone)]
pub enum ElectionError {
    /// Already a leader.
    AlreadyLeader,
    /// Not currently a leader.
    NotLeader,
    /// Election failed.
    Failed(String),
}

/// Result type for leader election operations.
pub type ElectionResult<T> = Result<T, ElectionError>;

/// Trait for leader election strategies.
#[async_trait::async_trait]
pub trait LeaderElection: Send + Sync + 'static {
    /// Attempt to become the leader.
    async fn become_leader(&mut self) -> ElectionResult<()>;

    /// Check if currently the leader.
    async fn is_leader(&self) -> bool;

    /// Resign leadership.
    async fn resign(&mut self) -> ElectionResult<()>;

    /// Get the current leader ID.
    async fn get_leader(&self) -> Option<String>;

    /// Refresh the lock so it doesn't expire. No-op for impls that don't expire.
    async fn renew(&mut self) -> ElectionResult<()> {
        Ok(())
    }
}

/// Spawn a background task that calls `election.renew()` every `period`.
///
/// The returned [`JoinHandle`] can be `.abort()`ed on shutdown. Renewal
/// failures are logged as warnings; the task keeps retrying until aborted.
pub fn spawn_renewal(election: Arc<Mutex<dyn LeaderElection>>, period: Duration) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);
        // Skip the immediate first tick so we don't renew before the lock is
        // even held.
        interval.tick().await;
        loop {
            interval.tick().await;
            let mut guard = election.lock().await;
            if let Err(e) = guard.renew().await {
                tracing::warn!("leader election renewal failed: {e:?}");
            }
        }
    })
}

/// Handle to leader status.
#[derive(Clone)]
pub struct LeaderHandle {
    election: Arc<Mutex<dyn LeaderElection>>,
}

impl std::fmt::Debug for LeaderHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LeaderHandle").finish()
    }
}

impl LeaderHandle {
    /// Create a new `LeaderHandle` wrapping the given election implementation.
    pub fn new(election: Arc<Mutex<dyn LeaderElection>>) -> Self {
        Self { election }
    }

    /// Convenience constructor that wraps `election` in `Arc<Mutex<…>>`.
    pub fn from_impl(election: impl LeaderElection) -> Self {
        Self {
            election: Arc::new(Mutex::new(election)),
        }
    }

    /// Returns `true` when the wrapped election impl reports we are leader.
    pub async fn is_leader(&self) -> bool {
        self.election.lock().await.is_leader().await
    }

    /// Spawn a background renewal task for this handle.
    ///
    /// Calls `renew()` every `period`. Abort the returned handle on shutdown.
    pub fn start_renewal_task(&self, period: Duration) -> JoinHandle<()> {
        spawn_renewal(Arc::clone(&self.election), period)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::g2v::election::memory::MemoryElection;

    #[tokio::test]
    async fn test_renew_default_returns_ok() {
        let mut election = MemoryElection::new("test-key", "inst-1");
        // Default renew impl must succeed.
        election
            .renew()
            .await
            .expect("default renew should return Ok");
    }

    #[tokio::test]
    async fn test_leader_handle_from_impl() {
        let mut election = MemoryElection::new("test-key", "inst-1");
        election.become_leader().await.unwrap();
        let handle = LeaderHandle::from_impl(election);
        assert!(handle.is_leader().await);
    }
}
