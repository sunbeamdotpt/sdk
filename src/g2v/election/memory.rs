//! In-memory leader election for testing.

use crate::g2v::election::{ElectionError, ElectionResult, LeaderElection};
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory leader election.
///
/// This is primarily useful for testing and development.
/// It uses a shared atomic value to track the current leader.
#[derive(Debug, Clone)]
pub struct MemoryElection {
    /// The election key (used for identifying the election).
    election_key: String,
    /// Local instance ID.
    instance_id: String,
    /// Shared state.
    state: Arc<MemoryElectionState>,
}

/// Shared state for memory election.
#[derive(Debug)]
pub struct MemoryElectionState {
    leader_id: RwLock<Option<String>>,
}

impl MemoryElectionState {
    fn new() -> Self {
        Self {
            leader_id: RwLock::new(None),
        }
    }

    async fn get_leader(&self) -> Option<String> {
        self.leader_id.read().await.clone()
    }

    async fn set_leader(&self, leader: Option<String>) {
        *self.leader_id.write().await = leader;
    }
}

impl MemoryElection {
    /// Create a new in-memory leader election.
    pub fn new(election_key: impl Into<String>, instance_id: impl Into<String>) -> Self {
        Self {
            election_key: election_key.into(),
            instance_id: instance_id.into(),
            state: Arc::new(MemoryElectionState::new()),
        }
    }

    /// Create a shared in-memory election (for testing with multiple instances).
    pub fn with_shared_state(
        election_key: impl Into<String>,
        instance_id: impl Into<String>,
        state: Arc<MemoryElectionState>,
    ) -> Self {
        Self {
            election_key: election_key.into(),
            instance_id: instance_id.into(),
            state,
        }
    }

    /// Get the election key.
    pub fn election_key(&self) -> &str {
        &self.election_key
    }

    /// Get the instance ID.
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

#[async_trait::async_trait]
impl LeaderElection for MemoryElection {
    async fn become_leader(&mut self) -> ElectionResult<()> {
        let current_leader = self.state.get_leader().await;

        // If there's already a leader, we can't become leader
        if current_leader.is_some() {
            return Err(ElectionError::AlreadyLeader);
        }

        // Become the leader
        self.state.set_leader(Some(self.instance_id.clone())).await;
        Ok(())
    }

    async fn is_leader(&self) -> bool {
        let current_leader = self.state.get_leader().await;
        current_leader == Some(self.instance_id.clone())
    }

    async fn resign(&mut self) -> ElectionResult<()> {
        let current_leader = self.state.get_leader().await;

        // Only the current leader can resign
        if current_leader != Some(self.instance_id.clone()) {
            return Err(ElectionError::NotLeader);
        }

        self.state.set_leader(None).await;
        Ok(())
    }

    async fn get_leader(&self) -> Option<String> {
        self.state.get_leader().await
    }
}

/// Create a new shared memory election state for testing.
pub fn new_shared_election() -> Arc<MemoryElectionState> {
    Arc::new(MemoryElectionState::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_election_become_leader() {
        let shared_state = new_shared_election();
        let mut election =
            MemoryElection::with_shared_state("test", "instance-1", shared_state.clone());

        assert!(!election.is_leader().await);
        election.become_leader().await.unwrap();
        assert!(election.is_leader().await);
    }

    #[tokio::test]
    async fn test_memory_election_only_one_leader() {
        let shared_state = new_shared_election();
        let mut election1 =
            MemoryElection::with_shared_state("test", "instance-1", shared_state.clone());
        let mut election2 =
            MemoryElection::with_shared_state("test", "instance-2", shared_state.clone());

        election1.become_leader().await.unwrap();
        assert!(election1.is_leader().await);
        assert!(!election2.is_leader().await);

        // instance-2 should fail to become leader
        let result = election2.become_leader().await;
        assert!(matches!(result, Err(ElectionError::AlreadyLeader)));
    }

    #[tokio::test]
    async fn test_memory_election_resign() {
        let shared_state = new_shared_election();
        let mut election =
            MemoryElection::with_shared_state("test", "instance-1", shared_state.clone());

        election.become_leader().await.unwrap();
        assert!(election.is_leader().await);

        election.resign().await.unwrap();
        assert!(!election.is_leader().await);
    }

    #[tokio::test]
    async fn test_memory_election_get_leader() {
        let shared_state = new_shared_election();
        let mut election1 =
            MemoryElection::with_shared_state("test", "instance-1", shared_state.clone());
        let election2 =
            MemoryElection::with_shared_state("test", "instance-2", shared_state.clone());

        election1.become_leader().await.unwrap();
        assert_eq!(election2.get_leader().await, Some("instance-1".to_string()));
    }

    #[tokio::test]
    async fn test_memory_election_not_leader_resign() {
        let shared_state = new_shared_election();
        let mut election1 =
            MemoryElection::with_shared_state("test", "instance-1", shared_state.clone());
        let mut election2 =
            MemoryElection::with_shared_state("test", "instance-2", shared_state.clone());

        election1.become_leader().await.unwrap();

        // instance-2 should fail to resign
        let result = election2.resign().await;
        assert!(matches!(result, Err(ElectionError::NotLeader)));
    }
}
