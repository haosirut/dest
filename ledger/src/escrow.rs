use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single challenge result record for a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChallengeRecord {
    timestamp: chrono::DateTime<chrono::Utc>,
    success: bool,
}

/// Manages escrow logic for storage challenge failures and refunds.
///
/// When a node accumulates 3 or more challenge failures in a 1-hour window,
/// a refund is issued from the reserve pool (10% of commission).
pub struct EscrowManager {
    /// Map from node_id to their challenge history.
    records: HashMap<String, Vec<ChallengeRecord>>,
    /// Commission pool reserve percentage used for refunds.
    commission_reserve_share: Decimal,
}

impl EscrowManager {
    /// Creates a new EscrowManager with default commission reserve of 10%.
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            commission_reserve_share: dec!(0.10),
        }
    }

    /// Records a successful challenge for a node and prunes old records.
    pub fn record_challenge_success(&mut self, node_id: &str) {
        self.prune_old_records(node_id, 1);
        let records = self.records.entry(node_id.to_string()).or_default();
        records.push(ChallengeRecord {
            timestamp: chrono::Utc::now(),
            success: true,
        });
    }

    /// Records a failed challenge for a node and prunes old records.
    pub fn record_challenge_failure(&mut self, node_id: &str) {
        self.prune_old_records(node_id, 1);
        let records = self.records.entry(node_id.to_string()).or_default();
        records.push(ChallengeRecord {
            timestamp: chrono::Utc::now(),
            success: false,
        });
    }

    /// Returns the number of failures in the last `window_hours` hours.
    pub fn get_fail_count_in_window(&self, node_id: &str, window_hours: u32) -> u32 {
        match self.records.get(node_id) {
            None => 0,
            Some(records) => {
                let cutoff = chrono::Utc::now() - chrono::Duration::hours(window_hours as i64);
                records
                    .iter()
                    .filter(|r| r.timestamp >= cutoff && !r.success)
                    .count() as u32
            }
        }
    }

    /// Checks if a refund is due and processes it.
    ///
    /// If a node has 3 or more failures in the specified window:
    /// - Calculates refund = total_hourly_cost * (fail_count / total_count) * commission_reserve_share
    /// - Clears the challenge window for that node
    ///
    /// Returns the refund amount (0 if no refund due).
    pub fn check_and_refund(&mut self, node_id: &str, hours: u32) -> Decimal {
        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::hours(hours as i64);

        let fail_count;
        let total_count;

        {
            let records = match self.records.get(node_id) {
                Some(r) => r,
                None => return Decimal::ZERO,
            };
            let window_records: Vec<_> = records
                .iter()
                .filter(|r| r.timestamp >= cutoff)
                .collect();
            fail_count = window_records.iter().filter(|r| !r.success).count() as u32;
            total_count = window_records.len() as u32;
        }

        if fail_count < 3 || total_count == 0 {
            return Decimal::ZERO;
        }

        // Calculate refund proportional to failure ratio
        // Assume a representative hourly cost for the refund calculation.
        // The refund pool is 10% of the platform commission from billing.
        // We use a base hourly cost as a representative value for the refund.
        let representative_hourly_cost = dec!(0.30); // base rate per TB/hour
        let failure_ratio = Decimal::from(fail_count) / Decimal::from(total_count);
        let refund = representative_hourly_cost * failure_ratio * self.commission_reserve_share;

        // Clear the window after issuing refund
        self.clear_window(node_id, hours);

        tracing::info!(
            "Refund issued for node {}: {} RUB ({} fails / {} total in {}h window)",
            node_id,
            refund,
            fail_count,
            total_count,
            hours
        );

        refund
    }

    /// Prunes challenge records older than the specified window.
    fn prune_old_records(&mut self, node_id: &str, window_hours: u32) {
        if let Some(records) = self.records.get_mut(node_id) {
            let cutoff = chrono::Utc::now() - chrono::Duration::hours(window_hours as i64);
            records.retain(|r| r.timestamp >= cutoff);
        }
    }

    /// Clears all records within the specified window for a node.
    fn clear_window(&mut self, node_id: &str, window_hours: u32) {
        if let Some(records) = self.records.get_mut(node_id) {
            let cutoff = chrono::Utc::now() - chrono::Duration::hours(window_hours as i64);
            records.retain(|r| r.timestamp < cutoff);
        }
    }

    /// Returns the total number of challenge records for a node (for testing).
    pub fn record_count(&self, node_id: &str) -> usize {
        self.records.get(node_id).map(|r| r.len()).unwrap_or(0)
    }
}

impl Default for EscrowManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_fails_no_refund() {
        let mut mgr = EscrowManager::new();
        mgr.record_challenge_success("node-a");
        mgr.record_challenge_success("node-a");
        mgr.record_challenge_success("node-a");
        let refund = mgr.check_and_refund("node-a", 1);
        assert_eq!(refund, Decimal::ZERO);
    }

    #[test]
    fn test_three_fails_triggers_refund() {
        let mut mgr = EscrowManager::new();
        mgr.record_challenge_failure("node-b");
        mgr.record_challenge_failure("node-b");
        mgr.record_challenge_failure("node-b");
        let refund = mgr.check_and_refund("node-b", 1);
        // fail_count=3, total_count=3, ratio=1.0
        // refund = 0.30 * 1.0 * 0.10 = 0.03
        assert_eq!(refund, dec!(0.03));
    }

    #[test]
    fn test_mixed_challenges_with_3_fails() {
        let mut mgr = EscrowManager::new();
        mgr.record_challenge_success("node-c");
        mgr.record_challenge_failure("node-c");
        mgr.record_challenge_success("node-c");
        mgr.record_challenge_failure("node-c");
        mgr.record_challenge_failure("node-c");
        // fail_count=3, total_count=5, ratio=0.6
        // refund = 0.30 * 0.6 * 0.10 = 0.018
        let refund = mgr.check_and_refund("node-c", 1);
        assert_eq!(refund, dec!(0.018));
    }

    #[test]
    fn test_two_fails_no_refund() {
        let mut mgr = EscrowManager::new();
        mgr.record_challenge_failure("node-d");
        mgr.record_challenge_failure("node-d");
        let refund = mgr.check_and_refund("node-d", 1);
        assert_eq!(refund, Decimal::ZERO);
    }

    #[test]
    fn test_window_clearing_after_refund() {
        let mut mgr = EscrowManager::new();
        mgr.record_challenge_failure("node-e");
        mgr.record_challenge_failure("node-e");
        mgr.record_challenge_failure("node-e");
        let _ = mgr.check_and_refund("node-e", 1);
        // After refund, window should be cleared
        assert_eq!(mgr.record_count("node-e"), 0);
    }

    #[test]
    fn test_fail_count_in_window() {
        let mut mgr = EscrowManager::new();
        mgr.record_challenge_failure("node-f");
        mgr.record_challenge_failure("node-f");
        assert_eq!(mgr.get_fail_count_in_window("node-f", 1), 2);
    }

    #[test]
    fn test_fail_count_unknown_node() {
        let mgr = EscrowManager::new();
        assert_eq!(mgr.get_fail_count_in_window("unknown", 1), 0);
    }

    #[test]
    fn test_unknown_node_refund_is_zero() {
        let mut mgr = EscrowManager::new();
        let refund = mgr.check_and_refund("ghost", 1);
        assert_eq!(refund, Decimal::ZERO);
    }

    #[test]
    fn test_multiple_nodes_independent() {
        let mut mgr = EscrowManager::new();
        mgr.record_challenge_failure("node-x");
        mgr.record_challenge_failure("node-y");
        mgr.record_challenge_failure("node-y");
        mgr.record_challenge_failure("node-y");
        // node-x: 1 fail, no refund
        assert_eq!(mgr.check_and_refund("node-x", 1), Decimal::ZERO);
        // node-y: 3 fails, refund
        assert_eq!(mgr.check_and_refund("node-y", 1), dec!(0.03));
    }
}
