#[cfg(test)]
mod tests {
    #[test]
    fn test_reputation_success_below_one() {
        let mut rep = vaultkeeper_ledger::ReputationManager::new();
        // Simulate many failures to bring score below 1.0
        for _ in 0..50 { rep.record_failure("node1"); }
        let score = rep.get_score("node1").unwrap();
        assert_eq!(score, rust_decimal::Decimal::ZERO); // clamped to 0

        rep.record_success("node1"); // should boost
        let boosted = rep.get_score("node1").unwrap();
        assert_eq!(boosted, rust_decimal::Decimal::from_str("0.01").unwrap());
    }

    #[test]
    fn test_reputation_ban_at_16() {
        let mut rep = vaultkeeper_ledger::ReputationManager::new();
        for _ in 0..15 { rep.record_failure("node1"); }
        assert!(!rep.is_banned("node1"));
        rep.record_failure("node1");
        assert!(rep.is_banned("node1"));
    }

    #[test]
    fn test_im_back_resets_fails() {
        let mut rep = vaultkeeper_ledger::ReputationManager::new();
        for _ in 0..10 { rep.record_failure("node1"); }
        assert_eq!(rep.get_consecutive_fails("node1").unwrap(), 10);
        rep.handle_im_back("node1");
        assert_eq!(rep.get_consecutive_fails("node1").unwrap(), 0);
        assert!(!rep.is_banned("node1")); // 10 fails reset to 0, not banned
    }

    #[test]
    fn test_escrow_3_fails_triggers_refund() {
        let mut escrow = vaultkeeper_ledger::EscrowManager::new();
        // Record 3 failures in 1 hour
        escrow.record_challenge_failure("host1");
        escrow.record_challenge_failure("host1");
        escrow.record_challenge_failure("host1");
        let refund = escrow.check_and_refund("host1", 1);
        assert!(refund > rust_decimal::Decimal::ZERO);
    }

    #[test]
    fn test_ledger_offline_sync() {
        let store = vaultkeeper_ledger::store::LedgerStore::open_in_memory().unwrap();
        store.record_entry("deposit", "acc1", "100.00", "100.00").unwrap();
        store.record_entry("charge", "acc1", "10.00", "90.00").unwrap();
        assert_eq!(store.get_unsynced().unwrap().len(), 2);
        store.mark_synced(2).unwrap();
        assert_eq!(store.get_unsynced().unwrap().len(), 0);
    }
}
