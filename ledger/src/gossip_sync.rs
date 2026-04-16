use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Messages exchanged during gossip-based ledger synchronization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    /// Advertises a peer's current ledger state (latest seq, merkle root).
    StateAdvert {
        peer_id: String,
        latest_seq: u64,
        merkle_root: String,
    },
    /// Requests ledger entries starting from a given sequence number.
    EntryRequest {
        requester_id: String,
        from_seq: u64,
    },
    /// Response containing ledger entries.
    EntryResponse {
        responder_id: String,
        entries: Vec<SyncEntry>,
    },
    /// Notification about a detected conflict between ledger states.
    ConflictNotify {
        reporter_id: String,
        local_seq: u64,
        remote_seq: u64,
        local_merkle: String,
        remote_merkle: String,
    },
}

/// A simplified ledger entry for synchronization purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEntry {
    pub seq: u64,
    pub account_id: String,
    pub entry_type: String,
    pub amount: String,
    pub data_json: String,
}

/// Tracks the known state of a remote peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PeerState {
    latest_seq: u64,
    merkle_root: String,
    last_seen: chrono::DateTime<chrono::Utc>,
}

/// Manages gossip-based synchronization of the distributed ledger.
pub struct LedgerSyncManager {
    /// Local ledger's latest sequence number.
    local_latest_seq: u64,
    /// Local ledger's latest Merkle root.
    local_merkle_root: String,
    /// Known states of remote peers.
    peers: HashMap<String, PeerState>,
}

impl LedgerSyncManager {
    /// Creates a new sync manager with empty local state.
    pub fn new() -> Self {
        Self {
            local_latest_seq: 0,
            local_merkle_root: hex::encode(blake3::hash(b"").as_bytes()),
            peers: HashMap::new(),
        }
    }

    /// Creates a new sync manager with specific initial state.
    pub fn with_state(latest_seq: u64, merkle_root: &str) -> Self {
        Self {
            local_latest_seq: latest_seq,
            local_merkle_root: merkle_root.to_string(),
            peers: HashMap::new(),
        }
    }

    /// Registers or updates a remote peer's advertised state.
    pub fn register_peer_state(
        &mut self,
        peer_id: &str,
        latest_seq: u64,
        merkle_root: &str,
    ) {
        self.peers.insert(
            peer_id.to_string(),
            PeerState {
                latest_seq,
                merkle_root: merkle_root.to_string(),
                last_seen: chrono::Utc::now(),
            },
        );
    }

    /// Updates the local ledger state (e.g., after adding entries or computing new root).
    pub fn update_local_state(&mut self, latest_seq: u64, merkle_root: &str) {
        self.local_latest_seq = latest_seq;
        self.local_merkle_root = merkle_root.to_string();
    }

    /// Checks whether synchronization is needed with any known peer.
    /// Returns true if any peer has entries we don't have (peer.seq > local.seq).
    pub fn needs_sync(&self) -> bool {
        self.peers.values().any(|p| p.latest_seq > self.local_latest_seq)
    }

    /// Returns a list of peer IDs that are ahead of our local state.
    pub fn peers_ahead(&self) -> Vec<String> {
        self.peers
            .iter()
            .filter(|(_, p)| p.latest_seq > self.local_latest_seq)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Checks whether our local state is ahead of a specific peer.
    pub fn is_ahead_of(&self, peer_id: &str) -> bool {
        self.local_latest_seq
            > self.peers
                .get(peer_id)
                .map(|p| p.latest_seq)
                .unwrap_or(0)
    }

    /// Creates an EntryRequest message for catching up with the best peer.
    pub fn create_entry_request(&self) -> Option<SyncMessage> {
        // Find the peer with the highest seq that's ahead of us
        let best_peer = self
            .peers
            .iter()
            .filter(|(_, p)| p.latest_seq > self.local_latest_seq)
            .max_by_key(|(_, p)| p.latest_seq);

        match best_peer {
            Some((_peer_id, _)) => Some(SyncMessage::EntryRequest {
                requester_id: "self".to_string(),
                from_seq: self.local_latest_seq,
            }),
            None => None,
        }
    }

    /// Creates a StateAdvert message advertising our local state.
    pub fn create_state_advert(&self, self_id: &str) -> SyncMessage {
        SyncMessage::StateAdvert {
            peer_id: self_id.to_string(),
            latest_seq: self.local_latest_seq,
            merkle_root: self.local_merkle_root.clone(),
        }
    }

    /// Processes incoming entries from a peer response.
    /// Returns the number of entries that are new (seq > local_latest_seq).
    pub fn process_entries(&mut self, entries: &[SyncEntry]) -> u32 {
        let mut new_count = 0u32;
        for entry in entries {
            if entry.seq as u64 > self.local_latest_seq {
                new_count += 1;
                self.local_latest_seq = entry.seq as u64;
            }
        }
        // Recompute merkle root after processing
        // (In a real implementation this would use the actual ledger data)
        self.local_merkle_root = hex::encode(
            blake3::hash(format!("seq:{}", self.local_latest_seq).as_bytes()).as_bytes(),
        );
        new_count
    }

    /// Returns the current local state.
    pub fn local_state(&self) -> (u64, String) {
        (self.local_latest_seq, self.local_merkle_root.clone())
    }

    /// Returns the number of known peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Detects conflicts between local and a specific peer's state.
    /// A conflict exists if both sides have entries the other doesn't.
    pub fn detect_conflict(&self, peer_id: &str) -> Option<SyncMessage> {
        let peer = self.peers.get(peer_id)?;
        // Conflict: peer has entries we don't AND we have entries they don't
        let peer_ahead = peer.latest_seq > self.local_latest_seq;
        let we_ahead = self.local_latest_seq > peer.latest_seq;
        // Divergent conflict: different merkle roots at the same seq
        let divergent = peer.latest_seq == self.local_latest_seq
            && peer.merkle_root != self.local_merkle_root;

        if peer_ahead || we_ahead || divergent {
            Some(SyncMessage::ConflictNotify {
                reporter_id: "self".to_string(),
                local_seq: self.local_latest_seq,
                remote_seq: peer.latest_seq,
                local_merkle: self.local_merkle_root.clone(),
                remote_merkle: peer.merkle_root.clone(),
            })
        } else {
            None
        }
    }
}

impl Default for LedgerSyncManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_sync_manager() {
        let mgr = LedgerSyncManager::new();
        let (seq, root) = mgr.local_state();
        assert_eq!(seq, 0);
        assert!(!root.is_empty());
    }

    #[test]
    fn test_register_peer() {
        let mut mgr = LedgerSyncManager::new();
        mgr.register_peer_state("peer-1", 10, "hash-abc");
        assert_eq!(mgr.peer_count(), 1);
    }

    #[test]
    fn test_update_local_state() {
        let mut mgr = LedgerSyncManager::new();
        mgr.update_local_state(5, "new-hash");
        let (seq, root) = mgr.local_state();
        assert_eq!(seq, 5);
        assert_eq!(root, "new-hash");
    }

    #[test]
    fn test_needs_sync_false_when_no_peers() {
        let mgr = LedgerSyncManager::new();
        assert!(!mgr.needs_sync());
    }

    #[test]
    fn test_needs_sync_true_when_peer_ahead() {
        let mut mgr = LedgerSyncManager::new();
        mgr.register_peer_state("peer-1", 10, "hash");
        assert!(mgr.needs_sync());
    }

    #[test]
    fn test_needs_sync_false_when_peer_behind() {
        let mut mgr = LedgerSyncManager::with_state(20, "my-hash");
        mgr.register_peer_state("peer-1", 10, "hash");
        assert!(!mgr.needs_sync());
    }

    #[test]
    fn test_create_entry_request() {
        let mut mgr = LedgerSyncManager::new();
        assert!(mgr.create_entry_request().is_none());
        mgr.register_peer_state("peer-1", 10, "hash");
        let request = mgr.create_entry_request();
        assert!(request.is_some());
        match request.unwrap() {
            SyncMessage::EntryRequest { from_seq, .. } => assert_eq!(from_seq, 0),
            _ => panic!("Expected EntryRequest"),
        }
    }

    #[test]
    fn test_create_state_advert() {
        let mgr = LedgerSyncManager::with_state(5, "my-root");
        let advert = mgr.create_state_advert("node-me");
        match advert {
            SyncMessage::StateAdvert {
                peer_id,
                latest_seq,
                merkle_root,
            } => {
                assert_eq!(peer_id, "node-me");
                assert_eq!(latest_seq, 5);
                assert_eq!(merkle_root, "my-root");
            }
            _ => panic!("Expected StateAdvert"),
        }
    }

    #[test]
    fn test_process_entries() {
        let mut mgr = LedgerSyncManager::new();
        let entries = vec![
            SyncEntry {
                seq: 1,
                account_id: "a".into(),
                entry_type: "deposit".into(),
                amount: "100".into(),
                data_json: "{}".into(),
            },
            SyncEntry {
                seq: 2,
                account_id: "b".into(),
                entry_type: "charge".into(),
                amount: "50".into(),
                data_json: "{}".into(),
            },
        ];
        let count = mgr.process_entries(&entries);
        assert_eq!(count, 2);
        assert_eq!(mgr.local_state().0, 2);
    }

    #[test]
    fn test_process_entries_dedup() {
        let mut mgr = LedgerSyncManager::with_state(5, "hash");
        let entries = vec![
            SyncEntry {
                seq: 3,
                account_id: "a".into(),
                entry_type: "x".into(),
                amount: "1".into(),
                data_json: "{}".into(),
            },
            SyncEntry {
                seq: 6,
                account_id: "b".into(),
                entry_type: "x".into(),
                amount: "2".into(),
                data_json: "{}".into(),
            },
        ];
        let count = mgr.process_entries(&entries);
        assert_eq!(count, 1); // only seq=6 is new
        assert_eq!(mgr.local_state().0, 6);
    }

    #[test]
    fn test_detect_conflict_divergent() {
        let mut mgr = LedgerSyncManager::with_state(5, "local-hash");
        mgr.register_peer_state("peer-x", 5, "different-hash");
        let conflict = mgr.detect_conflict("peer-x");
        assert!(conflict.is_some());
    }

    #[test]
    fn test_detect_no_conflict_same_state() {
        let mut mgr = LedgerSyncManager::with_state(5, "same-hash");
        mgr.register_peer_state("peer-x", 5, "same-hash");
        let conflict = mgr.detect_conflict("peer-x");
        assert!(conflict.is_none());
    }

    #[test]
    fn test_peers_ahead() {
        let mut mgr = LedgerSyncManager::with_state(3, "h");
        mgr.register_peer_state("peer-a", 5, "h1");
        mgr.register_peer_state("peer-b", 2, "h2");
        mgr.register_peer_state("peer-c", 10, "h3");
        let ahead = mgr.peers_ahead();
        assert_eq!(ahead.len(), 2);
        assert!(ahead.contains(&"peer-a".to_string()));
        assert!(ahead.contains(&"peer-c".to_string()));
    }
}
