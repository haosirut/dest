//! Replication logic — auto-replica on node failure within 10 minutes.
//!
//! The `ReplicationManager` tracks which peers hold shards for each chunk
//! and ensures the target replication factor is met. When a peer dies,
//! affected chunks are flagged for re-replication.
//!
//! On mobile platforms, the replication manager still works for tracking
//! (clients can monitor replication state) but hosting is disabled.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::{info, warn};

/// Replication state for a single chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationState {
    /// Content-addressable chunk identifier.
    pub chunk_id: String,
    /// Target number of replicas.
    pub target_replicas: u32,
    /// Map of peer_id -> number of shards held by that peer.
    pub current_replicas: HashMap<String, usize>,
    /// Whether the chunk has sufficient replication.
    pub is_complete: bool,
}

impl ReplicationState {
    /// Create a new replication state for a chunk.
    pub fn new(chunk_id: String, target_replicas: u32) -> Self {
        Self {
            chunk_id,
            target_replicas,
            current_replicas: HashMap::new(),
            is_complete: false,
        }
    }

    /// Check if replication is sufficient.
    ///
    /// A chunk is considered replicated when the total number of replicas
    /// across all peers meets or exceeds the target.
    pub fn check_replication(&self) -> bool {
        let total_shards: usize = self.current_replicas.values().sum();
        total_shards >= self.target_replicas as usize
    }

    /// Add a replica for a peer.
    pub fn add_replica(&mut self, peer_id: &str, shard_count: usize) {
        self.current_replicas
            .insert(peer_id.to_string(), shard_count);
        self.is_complete = self.check_replication();
    }

    /// Remove replicas from a dead peer.
    ///
    /// Returns the number of shards that were held by the removed peer.
    pub fn remove_peer(&mut self, peer_id: &str) -> usize {
        let removed = self.current_replicas.remove(peer_id).unwrap_or(0);
        self.is_complete = self.check_replication();
        removed
    }

    /// Get peers currently holding replicas.
    pub fn peer_list(&self) -> Vec<&str> {
        self.current_replicas.keys().map(|s| s.as_str()).collect()
    }

    /// Get the total number of replicas across all peers.
    pub fn total_replicas(&self) -> usize {
        self.current_replicas.values().sum()
    }

    /// Get the number of distinct peers holding replicas.
    pub fn peer_count(&self) -> usize {
        self.current_replicas.len()
    }
}

/// Replication manager — tracks and triggers replication across the network.
pub struct ReplicationManager {
    /// Map of chunk_id -> ReplicationState.
    states: HashMap<String, ReplicationState>,
    /// Default target replica count for new chunks.
    default_replicas: u32,
}

impl ReplicationManager {
    /// Create a new replication manager.
    ///
    /// `default_replicas` is the target replication factor for newly
    /// registered chunks (typically 3).
    pub fn new(default_replicas: u32) -> Self {
        Self {
            states: HashMap::new(),
            default_replicas,
        }
    }

    /// Register a chunk for replication tracking.
    pub fn register_chunk(&mut self, chunk_id: &str) {
        let state = ReplicationState::new(chunk_id.to_string(), self.default_replicas);
        self.states.insert(chunk_id.to_string(), state);
        info!(
            "Registered chunk {} for replication (target: {})",
            chunk_id, self.default_replicas
        );
    }

    /// Register a chunk with a custom replication target.
    pub fn register_chunk_with_target(&mut self, chunk_id: &str, target: u32) {
        let state = ReplicationState::new(chunk_id.to_string(), target);
        self.states.insert(chunk_id.to_string(), state);
        info!(
            "Registered chunk {} for replication (target: {})",
            chunk_id, target
        );
    }

    /// Record that a peer holds shards for a chunk.
    pub fn record_peer_shards(&mut self, chunk_id: &str, peer_id: &str, shard_count: usize) {
        if let Some(state) = self.states.get_mut(chunk_id) {
            state.add_replica(peer_id, shard_count);
        }
    }

    /// Handle a dead peer — mark chunks as needing re-replication.
    ///
    /// Returns a list of chunk IDs that need re-replication due to
    /// the peer's death.
    pub fn handle_peer_death(&mut self, dead_peer_id: &str) -> Vec<String> {
        let mut chunks_needing_repair = Vec::new();

        for (chunk_id, state) in &mut self.states {
            let removed = state.remove_peer(dead_peer_id);
            if removed > 0 && !state.is_complete {
                chunks_needing_repair.push(chunk_id.clone());
                warn!(
                    "Chunk {} needs re-replication after peer {} death ({} shards lost)",
                    chunk_id, dead_peer_id, removed
                );
            }
        }

        chunks_needing_repair
    }

    /// Get all chunks that need replication (are not yet fully replicated).
    pub fn chunks_needing_replication(&self) -> Vec<&ReplicationState> {
        self.states.values().filter(|s| !s.is_complete).collect()
    }

    /// Get the chunk IDs that need replication.
    pub fn chunk_ids_needing_replication(&self) -> Vec<String> {
        self.states
            .iter()
            .filter(|(_, s)| !s.is_complete)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Check if a chunk has sufficient replication.
    pub fn is_replicated(&self, chunk_id: &str) -> bool {
        self.states
            .get(chunk_id)
            .map(|s| s.is_complete)
            .unwrap_or(false)
    }

    /// Remove a chunk from tracking.
    pub fn unregister_chunk(&mut self, chunk_id: &str) {
        self.states.remove(chunk_id);
    }

    /// Get the number of tracked chunks.
    pub fn tracked_count(&self) -> usize {
        self.states.len()
    }

    /// Get the number of fully replicated chunks.
    pub fn replicated_count(&self) -> usize {
        self.states.values().filter(|s| s.is_complete).count()
    }

    /// Get the replication state for a specific chunk.
    pub fn get_state(&self, chunk_id: &str) -> Option<&ReplicationState> {
        self.states.get(chunk_id)
    }

    /// Get all unique peer IDs across all tracked chunks.
    pub fn all_peers(&self) -> HashSet<String> {
        let mut peers = HashSet::new();
        for state in self.states.values() {
            for peer_id in state.current_replicas.keys() {
                peers.insert(peer_id.clone());
            }
        }
        peers
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
        assert_eq!(state.total_replicas(), 0);
        assert_eq!(state.peer_count(), 0);
    }

    #[test]
    fn test_replication_complete() {
        let mut state = ReplicationState::new("chunk1".into(), 3);
        state.add_replica("peer1", 1);
        state.add_replica("peer2", 1);
        state.add_replica("peer3", 1);
        assert!(state.is_complete);
        assert_eq!(state.total_replicas(), 3);
        assert_eq!(state.peer_count(), 3);
    }

    #[test]
    fn test_replication_exceeds_target() {
        let mut state = ReplicationState::new("chunk1".into(), 3);
        state.add_replica("peer1", 2);
        state.add_replica("peer2", 2);
        assert!(state.is_complete);
        assert_eq!(state.total_replicas(), 4);
    }

    #[test]
    fn test_replication_peer_death() {
        let mut state = ReplicationState::new("chunk1".into(), 3);
        state.add_replica("peer1", 1);
        state.add_replica("peer2", 1);
        state.add_replica("peer3", 1);
        assert!(state.is_complete);

        let removed = state.remove_peer("peer2");
        assert_eq!(removed, 1);
        assert!(!state.is_complete);
    }

    #[test]
    fn test_replication_remove_nonexistent_peer() {
        let mut state = ReplicationState::new("chunk1".into(), 3);
        let removed = state.remove_peer("ghost_peer");
        assert_eq!(removed, 0);
        assert!(!state.is_complete);
    }

    #[test]
    fn test_replication_peer_list() {
        let mut state = ReplicationState::new("chunk1".into(), 3);
        state.add_replica("peer1", 1);
        state.add_replica("peer2", 1);
        let peers = state.peer_list();
        assert_eq!(peers.len(), 2);
        assert!(peers.contains(&"peer1"));
        assert!(peers.contains(&"peer2"));
    }

    #[test]
    fn test_replication_state_serialization() {
        let mut state = ReplicationState::new("chunk1".into(), 3);
        state.add_replica("peer1", 2);
        state.add_replica("peer2", 1);

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: ReplicationState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.chunk_id, "chunk1");
        assert_eq!(deserialized.target_replicas, 3);
        assert_eq!(deserialized.current_replicas.len(), 2);
        assert!(deserialized.is_complete);
    }

    #[test]
    fn test_replication_manager_register() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk("chunk1");
        assert_eq!(manager.tracked_count(), 1);
        assert!(!manager.is_replicated("chunk1"));
    }

    #[test]
    fn test_replication_manager_register_with_target() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk_with_target("chunk1", 5);
        let state = manager.get_state("chunk1").unwrap();
        assert_eq!(state.target_replicas, 5);
    }

    #[test]
    fn test_replication_manager_complete() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk("chunk1");
        manager.record_peer_shards("chunk1", "peer1", 1);
        manager.record_peer_shards("chunk1", "peer2", 1);
        manager.record_peer_shards("chunk1", "peer3", 1);
        assert!(manager.is_replicated("chunk1"));
        assert_eq!(manager.replicated_count(), 1);
    }

    #[test]
    fn test_replication_manager_peer_death() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk("chunk1");
        manager.record_peer_shards("chunk1", "peer1", 1);
        manager.record_peer_shards("chunk1", "peer2", 1);
        manager.record_peer_shards("chunk1", "peer3", 1);

        let needs_repair = manager.handle_peer_death("peer2");
        assert_eq!(needs_repair.len(), 1);
        assert_eq!(needs_repair[0], "chunk1");
        assert!(!manager.is_replicated("chunk1"));
    }

    #[test]
    fn test_replication_manager_peer_death_no_impact() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk("chunk1");
        manager.record_peer_shards("chunk1", "peer1", 1);
        manager.record_peer_shards("chunk1", "peer2", 1);
        manager.record_peer_shards("chunk1", "peer3", 1);

        // Killing a peer that doesn't hold any shards should not affect anything
        let needs_repair = manager.handle_peer_death("ghost_peer");
        assert!(needs_repair.is_empty());
        assert!(manager.is_replicated("chunk1"));
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
        assert_eq!(needing[0].chunk_id, "chunk2");
    }

    #[test]
    fn test_chunk_ids_needing_replication() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk("chunk1");
        manager.register_chunk("chunk2");
        manager.register_chunk("chunk3");
        manager.record_peer_shards("chunk1", "peer1", 3);

        let ids = manager.chunk_ids_needing_replication();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"chunk2".to_string()));
        assert!(ids.contains(&"chunk3".to_string()));
    }

    #[test]
    fn test_unregister_chunk() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk("chunk1");
        assert_eq!(manager.tracked_count(), 1);
        manager.unregister_chunk("chunk1");
        assert_eq!(manager.tracked_count(), 0);
        assert!(!manager.is_replicated("chunk1"));
    }

    #[test]
    fn test_all_peers() {
        let mut manager = ReplicationManager::new(3);
        manager.register_chunk("chunk1");
        manager.register_chunk("chunk2");
        manager.record_peer_shards("chunk1", "peer1", 1);
        manager.record_peer_shards("chunk1", "peer2", 1);
        manager.record_peer_shards("chunk2", "peer2", 1);
        manager.record_peer_shards("chunk2", "peer3", 1);

        let peers = manager.all_peers();
        assert_eq!(peers.len(), 3);
        assert!(peers.contains("peer1"));
        assert!(peers.contains("peer2"));
        assert!(peers.contains("peer3"));
    }

    #[test]
    fn test_get_state_none() {
        let manager = ReplicationManager::new(3);
        assert!(manager.get_state("nonexistent").is_none());
    }

    #[test]
    fn test_multiple_chunks_peer_death() {
        let mut manager = ReplicationManager::new(2);
        manager.register_chunk("chunk1");
        manager.register_chunk("chunk2");
        manager.record_peer_shards("chunk1", "peer1", 1);
        manager.record_peer_shards("chunk1", "peer2", 1);
        manager.record_peer_shards("chunk2", "peer1", 1);
        manager.record_peer_shards("chunk2", "peer2", 1);

        // Both chunks are fully replicated
        assert!(manager.is_replicated("chunk1"));
        assert!(manager.is_replicated("chunk2"));

        // Kill peer1 — both chunks lose a replica
        let needs_repair = manager.handle_peer_death("peer1");
        assert_eq!(needs_repair.len(), 2);
        assert!(!manager.is_replicated("chunk1"));
        assert!(!manager.is_replicated("chunk2"));
    }
}
