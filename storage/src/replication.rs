//! Replication logic — auto-replica on node failure within 10 minutes.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::{info, warn};

/// Replication state for a chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationState {
    pub chunk_id: String,
    pub target_replicas: u32,
    pub current_replicas: HashMap<String, usize>, // peer_id -> shard_count
    pub is_complete: bool,
}

impl ReplicationState {
    pub fn new(chunk_id: String, target_replicas: u32) -> Self {
        Self {
            chunk_id,
            target_replicas,
            current_replicas: HashMap::new(),
            is_complete: false,
        }
    }

    /// Check if replication is sufficient
    pub fn check_replication(&self) -> bool {
        let total_shards: usize = self.current_replicas.values().sum();
        total_shards >= self.target_replicas as usize
    }

    /// Add a replica for a peer
    pub fn add_replica(&mut self, peer_id: &str, shard_count: usize) {
        self.current_replicas.insert(peer_id.to_string(), shard_count);
        self.is_complete = self.check_replication();
    }

    /// Remove replicas from a dead peer
    pub fn remove_peer(&mut self, peer_id: &str) -> usize {
        let removed = self.current_replicas.remove(peer_id).unwrap_or(0);
        self.is_complete = self.check_replication();
        removed
    }

    /// Get peers that need new replicas
    pub fn peers_needing_replication(&self) -> Vec<&str> {
        if self.is_complete {
            return vec![];
        }
        self.current_replicas.keys().map(|s| s.as_str()).collect()
    }
}

/// Replication manager — tracks and triggers replication
pub struct ReplicationManager {
    states: HashMap<String, ReplicationState>,
    default_replicas: u32,
}

impl ReplicationManager {
    pub fn new(default_replicas: u32) -> Self {
        Self {
            states: HashMap::new(),
            default_replicas,
        }
    }

    /// Register a chunk for replication tracking
    pub fn register_chunk(&mut self, chunk_id: &str) {
        let state = ReplicationState::new(chunk_id.to_string(), self.default_replicas);
        self.states.insert(chunk_id.to_string(), state);
        info!("Registered chunk {} for replication (target: {})", chunk_id, self.default_replicas);
    }

    /// Record that a peer holds shards for a chunk
    pub fn record_peer_shards(&mut self, chunk_id: &str, peer_id: &str, shard_count: usize) {
        if let Some(state) = self.states.get_mut(chunk_id) {
            state.add_replica(peer_id, shard_count);
        }
    }

    /// Handle a dead peer — mark chunks as needing re-replication
    pub fn handle_peer_death(&mut self, dead_peer_id: &str) -> Vec<String> {
        let mut chunks_needing_repair = Vec::new();

        for (chunk_id, state) in &mut self.states {
            let removed = state.remove_peer(dead_peer_id);
            if removed > 0 && !state.is_complete {
                chunks_needing_repair.push(chunk_id.clone());
                warn!("Chunk {} needs re-replication after peer {} death ({} shards lost)",
                    chunk_id, dead_peer_id, removed);
            }
        }

        chunks_needing_repair
    }

    /// Get all chunks that need replication
    pub fn chunks_needing_replication(&self) -> Vec<&ReplicationState> {
        self.states.values().filter(|s| !s.is_complete).collect()
    }

    /// Check if a chunk has sufficient replication
    pub fn is_replicated(&self, chunk_id: &str) -> bool {
        self.states.get(chunk_id).map(|s| s.is_complete).unwrap_or(false)
    }

    /// Remove chunk from tracking
    pub fn unregister_chunk(&mut self, chunk_id: &str) {
        self.states.remove(chunk_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replication_state_new() {
        let state = ReplicationState::new("chunk1".into(), 3);
        assert!(!state.is_complete);
        assert_eq!(state.target_replicas, 3);
    }

    #[test]
    fn test_replication_complete() {
        let mut state = ReplicationState::new("chunk1".into(), 3);
        state.add_replica("peer1", 1);
        state.add_replica("peer2", 1);
        state.add_replica("peer3", 1);
        assert!(state.is_complete);
    }

    #[test]
    fn test_replication_peer_death() {
        let mut state = ReplicationState::new("chunk1".into(), 3);
        state.add_replica("peer1", 1);
        state.add_replica("peer2", 1);
        state.add_replica("peer3", 1);
        assert!(state.is_complete);

        state.remove_peer("peer2");
        assert!(!state.is_complete);
    }

    #[test]
    fn test_replication_manager_register() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk("chunk1");
        assert!(!manager.is_replicated("chunk1"));
    }

    #[test]
    fn test_replication_manager_complete() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk("chunk1");
        manager.record_peer_shards("chunk1", "peer1", 1);
        manager.record_peer_shards("chunk1", "peer2", 1);
        manager.record_peer_shards("chunk1", "peer3", 1);
        assert!(manager.is_replicated("chunk1"));
    }

    #[test]
    fn test_replication_manager_peer_death() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk("chunk1");
        manager.record_peer_shards("chunk1", "peer1", 1);
        manager.record_peer_shards("chunk1", "peer2", 1);
        manager.record_peer_shards("chunk1", "peer3", 1);

        let needs_repair = manager.handle_peer_death("peer2");
        assert!(!needs_repair.is_empty());
        assert!(!manager.is_replicated("chunk1"));
    }

    #[test]
    fn test_needing_replication() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk("chunk1");
        manager.register_chunk("chunk2");
        manager.record_peer_shards("chunk1", "peer1", 3);
        manager.record_peer_shards("chunk2", "peer1", 1);

        let needing = manager.chunks_needing_replication();
        assert_eq!(needing.len(), 1);
    }

    #[test]
    fn test_unregister_chunk() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk("chunk1");
        manager.unregister_chunk("chunk1");
        assert!(!manager.is_replicated("chunk1"));
    }
}
