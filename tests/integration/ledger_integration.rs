//! Integration tests for ledger system

#[cfg(test)]
mod tests {
    #[test]
    fn test_ledger_offline_write_online_sync() {
        use vaultkeeper_ledger::LedgerStore;

        // Simulate offline: write entries
        let store = LedgerStore::open_in_memory().unwrap();
        store.record_entry("deposit", "acc1", "100.00", "100.00").unwrap();
        store.record_entry("charge", "acc1", "10.00", "90.00").unwrap();

        // Check unsynced
        let unsynced = store.get_unsynced().unwrap();
        assert_eq!(unsynced.len(), 2);

        // Compute Merkle root
        let root1 = store.compute_merkle_root().unwrap();

        // Simulate sync
        store.mark_synced(2).unwrap();
        let unsynced = store.get_unsynced().unwrap();
        assert_eq!(unsynced.len(), 0);

        // Root should be stable
        let root2 = store.compute_merkle_root().unwrap();
        assert_eq!(root1, root2);
    }

    #[test]
    fn test_conflict_resolution_integration() {
        use vaultkeeper_ledger::conflict::{ConflictResolver, LedgerConflict};
        use vaultkeeper_ledger::gossip_sync::LedgerSyncManager;

        let local = serde_json::json!({
            "id": "tx1", "seq": 1,
            "entry_type": "deposit", "amount": "100.00",
            "timestamp": "2024-01-01T00:00:00Z"
        });
        let remote = serde_json::json!({
            "id": "tx1", "seq": 1,
            "entry_type": "deposit", "amount": "200.00",
            "timestamp": "2024-01-01T01:00:00Z"
        });

        let conflict = LedgerConflict::new(local.clone(), remote.clone());
        assert!(conflict.resolve_by_timestamp()); // remote is newer

        // Test with majority
        assert!(!conflict.resolve_by_majority(5, 1)); // local wins by majority
        assert!(conflict.resolve_by_majority(1, 5)); // remote wins by majority

        // Test with sync manager
        let mut sync = LedgerSyncManager::new("peer1".to_string());
        sync.update_local_state("root_local", 1);
        sync.register_peer_state("peer2", "root_remote", 2);
        assert!(sync.needs_sync("peer2"));
    }
}
