use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Node operational status based on consecutive failure count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Node is operating normally (consecutive_fails < 10).
    Active,
    /// Node has accumulated warnings (10 <= consecutive_fails < 16).
    Warning,
    /// Node is banned (consecutive_fails >= 16).
    Banned,
}

/// Internal state tracking a single node's reputation.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReputationState {
    /// Reputation score. Starts at 5.0, range [0.0, 10.0].
    score: Decimal,
    /// Number of consecutive failures without a success.
    consecutive_fails: u32,
    /// Timestamp of last update.
    last_updated: chrono::DateTime<chrono::Utc>,
}

impl ReputationState {
    fn new() -> Self {
        Self {
            score: dec!(5.0),
            consecutive_fails: 0,
            last_updated: chrono::Utc::now(),
        }
    }
}

/// Ban threshold: 16 consecutive failures.
const BAN_THRESHOLD: u32 = 16;

/// Warning threshold: 10 consecutive failures.
const WARNING_THRESHOLD: u32 = 10;

/// Success recovery increment (only when score < 1.0).
const SUCCESS_RECOVERY: Decimal = dec!(0.01);

/// Failure penalty.
const FAILURE_PENALTY: Decimal = dec!(0.1);

/// Minimum score.
const MIN_SCORE: Decimal = dec!(0.0);

/// Maximum score.
const MAX_SCORE: Decimal = dec!(10.0);

/// Manages node reputation scores and statuses.
pub struct ReputationManager {
    /// Map from node_id to reputation state.
    nodes: HashMap<String, ReputationState>,
}

impl ReputationManager {
    /// Creates a new empty reputation manager.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    /// Returns the reputation score for a node, or None if never recorded.
    pub fn get_score(&self, node_id: &str) -> Option<Decimal> {
        self.nodes.get(node_id).map(|s| s.score)
    }

    /// Returns the number of consecutive failures for a node, or None if never recorded.
    pub fn get_consecutive_fails(&self, node_id: &str) -> Option<u32> {
        self.nodes.get(node_id).map(|s| s.consecutive_fails)
    }

    /// Returns the operational status of a node.
    /// - Active: consecutive_fails < 10
    /// - Warning: 10 <= consecutive_fails < 16
    /// - Banned: consecutive_fails >= 16
    pub fn get_status(&self, node_id: &str) -> NodeStatus {
        match self.nodes.get(node_id) {
            None => NodeStatus::Active,
            Some(state) => {
                if state.consecutive_fails >= BAN_THRESHOLD {
                    NodeStatus::Banned
                } else if state.consecutive_fails >= WARNING_THRESHOLD {
                    NodeStatus::Warning
                } else {
                    NodeStatus::Active
                }
            }
        }
    }

    /// Records a successful interaction for a node.
    ///
    /// - If score < 1.0: score += 0.01 (recovery boost, clamped to 10.0)
    /// - If score >= 1.0: no change to score
    /// - Always resets consecutive_fails to 0.
    pub fn record_success(&mut self, node_id: &str) {
        let state = self.nodes.entry(node_id.to_string()).or_insert_with(ReputationState::new);
        if state.score < dec!(1.0) {
            state.score = (state.score + SUCCESS_RECOVERY).min(MAX_SCORE);
        }
        state.consecutive_fails = 0;
        state.last_updated = chrono::Utc::now();
    }

    /// Records a failed interaction for a node.
    ///
    /// - score -= 0.1 (clamped to 0.0)
    /// - consecutive_fails += 1
    pub fn record_failure(&mut self, node_id: &str) {
        let state = self.nodes.entry(node_id.to_string()).or_insert_with(ReputationState::new);
        state.score = (state.score - FAILURE_PENALTY).max(MIN_SCORE);
        state.consecutive_fails += 1;
        state.last_updated = chrono::Utc::now();
    }

    /// Returns whether a node is banned (consecutive_fails >= 16).
    pub fn is_banned(&self, node_id: &str) -> bool {
        self.nodes
            .get(node_id)
            .map(|s| s.consecutive_fails >= BAN_THRESHOLD)
            .unwrap_or(false)
    }

    /// Handles a node's "I'm back" message by resetting consecutive_fails to 0.
    pub fn handle_im_back(&mut self, node_id: &str) {
        let state = self.nodes.entry(node_id.to_string()).or_insert_with(ReputationState::new);
        state.consecutive_fails = 0;
        state.last_updated = chrono::Utc::now();
    }
}

impl Default for ReputationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_score_is_five() {
        let mut mgr = ReputationManager::new();
        mgr.record_success("node-1"); // ensure entry exists
        assert_eq!(mgr.get_score("node-1"), Some(dec!(5.0)));
    }

    #[test]
    fn test_unknown_node_returns_none() {
        let mgr = ReputationManager::new();
        assert_eq!(mgr.get_score("unknown"), None);
        assert_eq!(mgr.get_consecutive_fails("unknown"), None);
    }

    #[test]
    fn test_unknown_node_status_is_active() {
        let mgr = ReputationManager::new();
        assert_eq!(mgr.get_status("unknown"), NodeStatus::Active);
    }

    #[test]
    fn test_success_boosts_score_below_one() {
        let mut mgr = ReputationManager::new();
        // First create a node with a low score via failures
        for _ in 0..50 {
            mgr.record_failure("node-low");
        }
        // Score should be clamped to 0.0
        assert_eq!(mgr.get_score("node-low"), Some(dec!(0.0)));

        // Now record success — should boost by 0.01
        mgr.record_success("node-low");
        assert_eq!(mgr.get_score("node-low"), Some(dec!(0.01)));
    }

    #[test]
    fn test_success_no_change_above_one() {
        let mut mgr = ReputationManager::new();
        mgr.record_success("node-ok");
        let score_before = mgr.get_score("node-ok").unwrap();
        mgr.record_success("node-ok");
        let score_after = mgr.get_score("node-ok").unwrap();
        assert_eq!(score_before, score_after);
    }

    #[test]
    fn test_failure_penalty() {
        let mut mgr = ReputationManager::new();
        mgr.record_success("node-f"); // initialize at 5.0
        mgr.record_failure("node-f");
        assert_eq!(mgr.get_score("node-f"), Some(dec!(4.9)));
        assert_eq!(mgr.get_consecutive_fails("node-f"), Some(1));
    }

    #[test]
    fn test_score_clamps_to_zero() {
        let mut mgr = ReputationManager::new();
        mgr.record_success("node-clamp"); // init at 5.0
        for _ in 0..100 {
            mgr.record_failure("node-clamp");
        }
        assert_eq!(mgr.get_score("node-clamp"), Some(dec!(0.0)));
    }

    #[test]
    fn test_recovery_stops_at_one_point_oh() {
        let mut mgr = ReputationManager::new();
        mgr.record_success("node-ten"); // init at 5.0
        // Drive score down to 0
        for _ in 0..100 {
            mgr.record_failure("node-ten");
        }
        assert_eq!(mgr.get_score("node-ten"), Some(dec!(0.0)));
        // Recovery only works while score < 1.0; each success adds 0.01.
        // After 100 successes from 0.0, score reaches 1.0 and stops growing.
        for _ in 0..100 {
            mgr.record_success("node-ten");
        }
        assert_eq!(mgr.get_score("node-ten"), Some(dec!(1.0)));
        // Further successes don't change score (already >= 1.0)
        mgr.record_success("node-ten");
        assert_eq!(mgr.get_score("node-ten"), Some(dec!(1.0)));
    }

    #[test]
    fn test_ban_at_16_consecutive_fails() {
        let mut mgr = ReputationManager::new();
        mgr.record_success("node-ban"); // initialize
        for i in 1..=15 {
            mgr.record_failure("node-ban");
            assert!(!mgr.is_banned("node-ban"), "should not be banned at fail {}", i);
        }
        mgr.record_failure("node-ban"); // 16th failure
        assert!(mgr.is_banned("node-ban"));
        assert_eq!(mgr.get_status("node-ban"), NodeStatus::Banned);
    }

    #[test]
    fn test_handle_im_back_resets_consecutive_fails() {
        let mut mgr = ReputationManager::new();
        mgr.record_success("node-back"); // initialize
        for _ in 0..16 {
            mgr.record_failure("node-back");
        }
        assert!(mgr.is_banned("node-back"));

        mgr.handle_im_back("node-back");
        assert_eq!(mgr.get_consecutive_fails("node-back"), Some(0));
        assert!(!mgr.is_banned("node-back"));
        // Score should still be low (not reset)
        assert_eq!(mgr.get_score("node-back"), Some(dec!(3.4))); // 5.0 - 16*0.1 = 3.4
    }

    #[test]
    fn test_success_resets_consecutive_fails() {
        let mut mgr = ReputationManager::new();
        mgr.record_success("node-reset"); // initialize
        for _ in 0..10 {
            mgr.record_failure("node-reset");
        }
        assert_eq!(mgr.get_consecutive_fails("node-reset"), Some(10));

        mgr.record_success("node-reset");
        assert_eq!(mgr.get_consecutive_fails("node-reset"), Some(0));
    }

    #[test]
    fn test_status_transitions() {
        let mut mgr = ReputationManager::new();
        mgr.record_success("node-t"); // initialize at 5.0

        // Active (0-9 fails)
        assert_eq!(mgr.get_status("node-t"), NodeStatus::Active);
        for _ in 0..9 {
            mgr.record_failure("node-t"); // 9 iterations → fails = 9
        }
        assert_eq!(mgr.get_status("node-t"), NodeStatus::Active);

        // Warning (10-15 fails)
        mgr.record_failure("node-t"); // 9 -> 10, reaches warning threshold
        assert_eq!(mgr.get_status("node-t"), NodeStatus::Warning);
        for _ in 0..5 {
            mgr.record_failure("node-t"); // 11..15
        }
        assert_eq!(mgr.get_status("node-t"), NodeStatus::Warning);

        // Banned (16+ fails)
        mgr.record_failure("node-t"); // 16
        assert_eq!(mgr.get_status("node-t"), NodeStatus::Banned);
    }
}
