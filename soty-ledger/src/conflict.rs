//! Conflict resolution for the distributed ledger.
//!
//! Strategy: timestamp + majority vote.
//! When two conflicting entries exist for the same sequence:
//! 1. Compare timestamps — newer wins
//! 2. If timestamps are equal, use majority vote among peers

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// A conflict between two ledger entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerConflict {
    pub local_entry: serde_json::Value,
    pub remote_entry: serde_json::Value,
    pub local_seq: i64,
    pub remote_seq: i64,
    pub local_timestamp: String,
    pub remote_timestamp: String,
}

impl LedgerConflict {
    /// Create a new conflict
    pub fn new(
        local_entry: serde_json::Value,
        remote_entry: serde_json::Value,
    ) -> Self {
        let local_seq = local_entry["seq"].as_i64().unwrap_or(0);
        let remote_seq = remote_entry["seq"].as_i64().unwrap_or(0);
        let local_timestamp = local_entry["timestamp"].as_str().unwrap_or("").to_string();
        let remote_timestamp = remote_entry["timestamp"].as_str().unwrap_or("").to_string();

        Self {
            local_entry,
            remote_entry,
            local_seq,
            remote_seq,
            local_timestamp,
            remote_timestamp,
        }
    }

    /// Resolve conflict by timestamp (newer wins).
    /// Returns true if remote entry should replace local.
    pub fn resolve_by_timestamp(&self) -> bool {
        // Remote is newer if its timestamp is after local's
        self.remote_timestamp > self.local_timestamp
    }

    /// Resolve by majority vote.
    /// `votes_for_local` and `votes_for_remote` are counts from other peers.
    /// Returns true if remote should replace local.
    pub fn resolve_by_majority(&self, votes_for_local: u32, votes_for_remote: u32) -> bool {
        if votes_for_remote > votes_for_local {
            return true;
        }
        if votes_for_local > votes_for_remote {
            return false;
        }
        // Tie: fall back to timestamp
        self.resolve_by_timestamp()
    }
}

/// Conflict resolver — manages resolution of ledger conflicts
pub struct ConflictResolver {
    resolved_count: u64,
    local_wins: u64,
    remote_wins: u64,
}

impl ConflictResolver {
    pub fn new() -> Self {
        Self {
            resolved_count: 0,
            local_wins: 0,
            remote_wins: 0,
        }
    }

    /// Detect conflicts between local and remote entries.
    /// A conflict exists when entries have the same ID but different content.
    pub fn detect_conflicts(
        &self,
        local_entries: &[serde_json::Value],
        remote_entries: &[serde_json::Value],
    ) -> Vec<LedgerConflict> {
        let mut conflicts = Vec::new();

        // Build a map of local entries by ID
        let local_map: std::collections::HashMap<String, &serde_json::Value> = local_entries
            .iter()
            .filter_map(|e| {
                let id = e["id"].as_str()?;
                Some((id.to_string(), e))
            })
            .collect();

        // Check remote entries against local
        for remote in remote_entries {
            if let Some(remote_id) = remote["id"].as_str() {
                if let Some(local) = local_map.get(remote_id) {
                    // Same ID but different content?
                    if local.to_string() != remote.to_string() {
                        conflicts.push(LedgerConflict::new((*local).clone(), remote.clone()));
                    }
                }
            }
        }

        debug!("Detected {} conflicts", conflicts.len());
        conflicts
    }

    /// Resolve a conflict using timestamp + majority vote.
    /// Returns the winning entry (local or remote).
    pub fn resolve(
        &mut self,
        conflict: &LedgerConflict,
        votes_for_local: u32,
        votes_for_remote: u32,
    ) -> ResolveResult {
        self.resolved_count += 1;

        // First try majority vote
        let use_remote = conflict.resolve_by_majority(votes_for_local, votes_for_remote);

        if use_remote {
            self.remote_wins += 1;
            info!("Conflict resolved: remote entry wins (seq={})", conflict.remote_seq);
            ResolveResult::UseRemote(conflict.remote_entry.clone())
        } else {
            self.local_wins += 1;
            info!("Conflict resolved: local entry wins (seq={})", conflict.local_seq);
            ResolveResult::UseLocal(conflict.local_entry.clone())
        }
    }

    /// Get resolution statistics
    pub fn stats(&self) -> (u64, u64, u64) {
        (self.resolved_count, self.local_wins, self.remote_wins)
    }
}

impl Default for ConflictResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of conflict resolution
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveResult {
    UseLocal(serde_json::Value),
    UseRemote(serde_json::Value),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(seq: i64, id: &str, timestamp: &str) -> serde_json::Value {
        serde_json::json!({
            "seq": seq,
            "id": id,
            "entry_type": "deposit",
            "account_id": "acc1",
            "amount": "100.00",
            "balance_after": "100.00",
            "timestamp": timestamp,
            "merkle_leaf": "leaf"
        })
    }

    #[test]
    fn test_no_conflict_same_entries() {
        let resolver = ConflictResolver::new();
        let entry = make_entry(1, "tx1", "2024-01-01T00:00:00Z");
        let conflicts = resolver.detect_conflicts(&[entry.clone()], &[entry]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_detect_conflict_different_content() {
        let resolver = ConflictResolver::new();
        let local = make_entry(1, "tx1", "2024-01-01T00:00:00Z");
        let remote = make_entry(1, "tx1", "2024-01-01T01:00:00Z");
        // Change a field to make them differ
        let mut remote = remote;
        remote["amount"] = serde_json::Value::String("200.00".to_string());

        let conflicts = resolver.detect_conflicts(&[local], &[remote]);
        assert_eq!(conflicts.len(), 1);
    }

    #[test]
    fn test_resolve_by_timestamp() {
        let local = make_entry(1, "tx1", "2024-01-01T00:00:00Z");
        let remote = make_entry(1, "tx1", "2024-01-01T01:00:00Z");

        let conflict = LedgerConflict::new(local, remote);
        // Remote is newer
        assert!(conflict.resolve_by_timestamp());
    }

    #[test]
    fn test_resolve_by_majority_remote_wins() {
        let local = make_entry(1, "tx1", "2024-01-01T00:00:00Z");
        let remote = make_entry(1, "tx1", "2024-01-01T01:00:00Z");

        let conflict = LedgerConflict::new(local, remote);
        assert!(conflict.resolve_by_majority(1, 3));
    }

    #[test]
    fn test_resolve_by_majority_local_wins() {
        let local = make_entry(1, "tx1", "2024-01-01T00:00:00Z");
        let remote = make_entry(1, "tx1", "2024-01-01T01:00:00Z");

        let conflict = LedgerConflict::new(local, remote);
        assert!(!conflict.resolve_by_majority(3, 1));
    }

    #[test]
    fn test_majority_tie_falls_back_to_timestamp() {
        let local = make_entry(1, "tx1", "2024-01-01T00:00:00Z");
        let remote = make_entry(1, "tx1", "2024-01-01T01:00:00Z");

        let conflict = LedgerConflict::new(local, remote);
        // Equal votes, remote is newer
        assert!(conflict.resolve_by_majority(2, 2));
    }

    #[test]
    fn test_resolver_stats() {
        let mut resolver = ConflictResolver::new();
        let local = make_entry(1, "tx1", "2024-01-01T00:00:00Z");
        let remote = make_entry(1, "tx1", "2024-01-01T01:00:00Z");

        let conflict = LedgerConflict::new(local, remote);
        resolver.resolve(&conflict, 1, 3);
        resolver.resolve(&conflict, 3, 1);

        let (total, local_wins, remote_wins) = resolver.stats();
        assert_eq!(total, 2);
        assert_eq!(local_wins, 1);
        assert_eq!(remote_wins, 1);
    }

    #[test]
    fn test_detect_multiple_conflicts() {
        let resolver = ConflictResolver::new();
        let local1 = make_entry(1, "tx1", "2024-01-01T00:00:00Z");
        let remote1 = make_entry(1, "tx1", "2024-01-01T01:00:00Z");
        let local2 = make_entry(2, "tx2", "2024-01-01T00:00:00Z");
        let remote2 = make_entry(2, "tx2", "2024-01-01T02:00:00Z");

        let conflicts = resolver.detect_conflicts(
            &[local1, local2.clone()],
            &[remote1, remote2],
        );
        // tx2 has no conflict (same content)
        assert_eq!(conflicts.len(), 2);
    }
}
