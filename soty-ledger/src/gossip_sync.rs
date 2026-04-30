//! Gossip-based ledger synchronization protocol.
//!
//! Designed for offline-first operation:
//! 1. Local writes happen immediately (SQLite)
//! 2. On reconnect, broadcast local Merkle root
//! 3. If root differs, exchange entry ranges
//! 4. Resolve conflicts (timestamp + majority)
//! 5. Apply winning entries

use crate::conflict::{ConflictResolver, ResolveResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

/// Sync message types for gossip protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    /// Advertise local ledger state
    StateAdvert {
        peer_id: String,
        merkle_root: String,
        latest_seq: i64,
    },
    /// Request entries from a given sequence
    EntryRequest {
        from_seq: i64,
        request_id: String,
    },
    /// Respond with entries
    EntryResponse {
        request_id: String,
        entries: Vec<serde_json::Value>,
    },
    /// Conflict notification
    ConflictNotify {
        conflicts: Vec<serde_json::Value>,
    },
}

/// Sync state tracker for a peer
#[derive(Debug, Clone)]
pub struct PeerSyncState {
    pub peer_id: String,
    pub merkle_root: String,
    pub latest_seq: i64,
    pub last_sync: u64,
}

/// Ledger sync manager
pub struct LedgerSyncManager {
    local_peer_id: String,
    local_merkle_root: String,
    local_latest_seq: i64,
    peers: HashMap<String, PeerSyncState>,
    conflict_resolver: ConflictResolver,
    pending_requests: HashMap<String, String>, // request_id -> peer_id
}

impl LedgerSyncManager {
    /// Create a new sync manager
    pub fn new(local_peer_id: String) -> Self {
        Self {
            local_peer_id,
            local_merkle_root: String::new(),
            local_latest_seq: 0,
            peers: HashMap::new(),
            conflict_resolver: ConflictResolver::new(),
            pending_requests: HashMap::new(),
        }
    }

    /// Update local ledger state (called after local writes)
    pub fn update_local_state(&mut self, merkle_root: &str, latest_seq: i64) {
        self.local_merkle_root = merkle_root.to_string();
        self.local_latest_seq = latest_seq;
        debug!("Local state updated: root={}, seq={}", merkle_root, latest_seq);
    }

    /// Register a peer's advertised state
    pub fn register_peer_state(&mut self, peer_id: &str, merkle_root: &str, latest_seq: i64) {
        let now = chrono::Utc::now().timestamp() as u64;

        self.peers.insert(peer_id.to_string(), PeerSyncState {
            peer_id: peer_id.to_string(),
            merkle_root: merkle_root.to_string(),
            latest_seq,
            last_sync: now,
        });

        debug!("Peer {} state: root={}, seq={}", peer_id, merkle_root, latest_seq);
    }

    /// Check if sync is needed with a peer
    pub fn needs_sync(&self, peer_id: &str) -> bool {
        if let Some(peer) = self.peers.get(peer_id) {
            if peer.merkle_root != self.local_merkle_root {
                return true;
            }
            // Also sync if peer has entries we don't
            if peer.latest_seq > self.local_latest_seq {
                return true;
            }
        }
        false
    }

    /// Create an entry request for a peer
    pub fn create_entry_request(&mut self, peer_id: &str) -> SyncMessage {
        let request_id = uuid::Uuid::new_v4().to_string();
        self.pending_requests.insert(request_id.clone(), peer_id.to_string());

        let from_seq = if let Some(peer) = self.peers.get(peer_id) {
            std::cmp::max(0, self.local_latest_seq - peer.latest_seq + 1)
        } else {
            0
        };

        SyncMessage::EntryRequest {
            from_seq,
            request_id,
        }
    }

    /// Create a state advertisement message
    pub fn create_state_advert(&self) -> SyncMessage {
        SyncMessage::StateAdvert {
            peer_id: self.local_peer_id.clone(),
            merkle_root: self.local_merkle_root.clone(),
            latest_seq: self.local_latest_seq,
        }
    }

    /// Process incoming entries from a peer
    pub fn process_entries(
        &mut self,
        request_id: &str,
        entries: Vec<serde_json::Value>,
        local_entries: &[serde_json::Value],
    ) -> Vec<ResolveResult> {
        let mut results = Vec::new();

        if let Some(peer_id) = self.pending_requests.remove(request_id) {
            // Detect conflicts
            let conflicts = self.conflict_resolver.detect_conflicts(local_entries, &entries);

            for conflict in &conflicts {
                // For now, use timestamp resolution (no majority votes in initial sync)
                let result = self.conflict_resolver.resolve(conflict, 0, 0);
                results.push(result);
            }

            if !conflicts.is_empty() {
                info!("Processed {} entries from {}, {} conflicts resolved",
                    entries.len(), peer_id, conflicts.len());
            } else {
                debug!("Processed {} entries from {}, no conflicts", entries.len(), peer_id);
            }
        }

        results
    }

    /// Get all peer states
    pub fn peer_states(&self) -> Vec<&PeerSyncState> {
        self.peers.values().collect()
    }

    /// Get conflict resolver stats
    pub fn conflict_stats(&self) -> (u64, u64, u64) {
        self.conflict_resolver.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_sync_manager() {
        let manager = LedgerSyncManager::new("peer1".to_string());
        assert_eq!(manager.local_peer_id, "peer1");
    }

    #[test]
    fn test_update_local_state() {
        let mut manager = LedgerSyncManager::new("peer1".to_string());
        manager.update_local_state("root_abc", 10);
        assert_eq!(manager.local_merkle_root, "root_abc");
        assert_eq!(manager.local_latest_seq, 10);
    }

    #[test]
    fn test_register_peer_state() {
        let mut manager = LedgerSyncManager::new("peer1".to_string());
        manager.register_peer_state("peer2", "root_xyz", 15);
        assert!(manager.peers.contains_key("peer2"));
    }

    #[test]
    fn test_needs_sync_different_roots() {
        let mut manager = LedgerSyncManager::new("peer1".to_string());
        manager.update_local_state("root_abc", 10);
        manager.register_peer_state("peer2", "root_xyz", 15);
        assert!(manager.needs_sync("peer2"));
    }

    #[test]
    fn test_no_sync_same_state() {
        let mut manager = LedgerSyncManager::new("peer1".to_string());
        manager.update_local_state("root_abc", 10);
        manager.register_peer_state("peer2", "root_abc", 10);
        assert!(!manager.needs_sync("peer2"));
    }

    #[test]
    fn test_create_state_advert() {
        let mut manager = LedgerSyncManager::new("peer1".to_string());
        manager.update_local_state("root_abc", 10);
        let advert = manager.create_state_advert();
        match advert {
            SyncMessage::StateAdvert { peer_id, merkle_root, latest_seq } => {
                assert_eq!(peer_id, "peer1");
                assert_eq!(merkle_root, "root_abc");
                assert_eq!(latest_seq, 10);
            }
            _ => panic!("Expected StateAdvert"),
        }
    }

    #[test]
    fn test_process_entries_no_conflicts() {
        let mut manager = LedgerSyncManager::new("peer1".to_string());
        let local_entries = vec![serde_json::json!({"id": "tx1", "seq": 1, "timestamp": "2024-01-01T00:00:00Z"})];
        let remote_entries = vec![serde_json::json!({"id": "tx2", "seq": 2, "timestamp": "2024-01-01T01:00:00Z"})];

        manager.pending_requests.insert("req1".to_string(), "peer2".to_string());
        let results = manager.process_entries("req1", remote_entries, &local_entries);
        assert!(results.is_empty());
    }

    #[test]
    fn test_process_entries_with_conflicts() {
        let mut manager = LedgerSyncManager::new("peer1".to_string());
        let local = serde_json::json!({"id": "tx1", "seq": 1, "timestamp": "2024-01-01T00:00:00Z", "amount": "100"});
        let remote = serde_json::json!({"id": "tx1", "seq": 1, "timestamp": "2024-01-01T01:00:00Z", "amount": "200"});

        manager.pending_requests.insert("req1".to_string(), "peer2".to_string());
        let results = manager.process_entries("req1", vec![remote], &[local]);
        assert_eq!(results.len(), 1);
    }
}
