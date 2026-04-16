use rust_decimal::Decimal;
use std::str::FromStr;

use crate::freeze::{FreezeManager, FreezeState};
use crate::rates::{self, DiskType};
use crate::subscription::{SubscriptionManager, SubscriptionTier};

/// Errors that can occur during billing operations.
#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: Decimal, available: Decimal },
    #[error("Account is frozen: {state:?}")]
    AccountFrozen { state: FreezeState },
    #[error("Invalid disk type: {0}")]
    InvalidDiskType(String),
    #[error("Invalid amount: {0}")]
    InvalidAmount(String),
    #[error("Subscription error: {0}")]
    SubscriptionError(String),
}

/// The main billing engine that combines account management, cost calculation,
/// freeze logic, and subscription management.
pub struct BillingEngine {
    /// Current account balance in RUB.
    balance: Decimal,
    /// Subscription manager handling tier and pro-rata calculations.
    subscription: SubscriptionManager,
    /// Freeze manager handling the account lifecycle (active/frozen/deleted).
    freeze: FreezeManager,
}

impl BillingEngine {
    /// Creates a new billing engine with zero balance and Archive tier.
    pub fn new() -> Self {
        Self {
            balance: Decimal::ZERO,
            subscription: SubscriptionManager::new(SubscriptionTier::Archive),
            freeze: FreezeManager::new(),
        }
    }

    /// Creates a new billing engine with a specific initial tier.
    pub fn with_tier(tier: SubscriptionTier) -> Self {
        Self {
            balance: Decimal::ZERO,
            subscription: SubscriptionManager::new(tier),
            freeze: FreezeManager::new(),
        }
    }

    /// Returns the current account balance.
    pub fn get_current_balance(&self) -> Decimal {
        self.balance
    }

    /// Deposits funds into the account. Unfreezes the account if it was frozen.
    pub fn deposit(&mut self, amount: Decimal) -> Result<(), BillingError> {
        if amount <= Decimal::ZERO {
            return Err(BillingError::InvalidAmount(format!(
                "Deposit amount must be positive, got {}",
                amount
            )));
        }
        self.balance += amount;
        // Unfreeze on any deposit
        if self.freeze.state() != FreezeState::Active {
            self.freeze.unfreeze();
            tracing::info!("Account unfrozen after deposit of {}", amount);
        }
        Ok(())
    }

    /// Deducts funds from the account. Triggers freeze if balance drops to zero or below.
    pub fn deduct(&mut self, amount: &Decimal) -> Result<(), BillingError> {
        if *amount <= Decimal::ZERO {
            return Err(BillingError::InvalidAmount(format!(
                "Deduct amount must be positive, got {}",
                amount
            )));
        }
        if self.freeze.state() != FreezeState::Active {
            return Err(BillingError::AccountFrozen {
                state: self.freeze.state(),
            });
        }
        if self.balance < *amount {
            return Err(BillingError::InsufficientBalance {
                required: *amount,
                available: self.balance,
            });
        }
        self.balance -= amount;
        // Trigger freeze if balance drops to zero or below
        if self.balance <= Decimal::ZERO {
            self.freeze.trigger_freeze();
            tracing::warn!("Balance depleted; account frozen");
        }
        Ok(())
    }

    /// Estimates the hourly cost for storing a file with given parameters.
    ///
    /// - `file_size_bytes`: Size of the file in bytes
    /// - `replication`: Number of replicas (2, 3, or 4)
    /// - `disk_type`: Storage disk type ("hdd", "ssd", or "nvme")
    /// - `cushion`: Whether to apply the 25% cushion multiplier
    pub fn estimate_upload_cost(
        &self,
        file_size_bytes: u64,
        replication: u8,
        disk_type: &str,
        cushion: bool,
    ) -> Result<Decimal, BillingError> {
        if file_size_bytes == 0 {
            return Err(BillingError::InvalidAmount(
                "File size must be positive".into(),
            ));
        }
        if !(2..=4).contains(&replication) {
            return Err(BillingError::InvalidAmount(format!(
                "Replication must be 2, 3, or 4, got {}",
                replication
            )));
        }
        let disk = DiskType::from_str(disk_type).map_err(|e| BillingError::InvalidDiskType(e.to_string()))?;

        // Calculate hourly rate per TB
        let hourly_rate_per_tb = rates::calculate_hourly_rate(disk, replication, cushion);

        // Convert file size to TB (as Decimal)
        let file_size_tb = Decimal::from(file_size_bytes) / Decimal::from(rates::BYTES_PER_TB);

        // Hourly cost = hourly_rate_per_tb * size_in_tb
        let hourly_cost = hourly_rate_per_tb * file_size_tb;

        // Round to 8 decimal places for precision
        Ok(hourly_cost.round_dp(8))
    }

    /// Sets a new subscription tier, returning the pro-rata net amount.
    ///
    /// Positive return = charge to user, negative = credit to user.
    pub fn set_subscription(&mut self, tier: SubscriptionTier) -> Result<Decimal, BillingError> {
        let net = self.subscription.change_tier(tier);
        tracing::info!(
            "Subscription changed to {}; pro-rata net: {}",
            tier,
            net
        );
        Ok(net)
    }

    /// Returns the current subscription tier.
    pub fn get_subscription(&self) -> SubscriptionTier {
        self.subscription.current_tier()
    }

    /// Returns whether the account is frozen (FrozenExport or HardDeleted).
    pub fn is_frozen(&self) -> bool {
        self.freeze.state() != FreezeState::Active
    }

    /// Returns the number of seconds remaining in the export window (0 if not frozen).
    pub fn remaining_export_seconds(&self) -> i64 {
        self.freeze.remaining_export_seconds()
    }

    /// Returns the current freeze state.
    pub fn get_freeze_state(&self) -> FreezeState {
        self.freeze.state()
    }
}

impl Default for BillingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_new_engine_zero_balance() {
        let engine = BillingEngine::new();
        assert_eq!(engine.get_current_balance(), Decimal::ZERO);
        assert_eq!(engine.get_subscription(), SubscriptionTier::Archive);
        assert!(!engine.is_frozen());
    }

    #[test]
    fn test_deposit_adds_funds() {
        let mut engine = BillingEngine::new();
        engine.deposit(dec!(100.0)).unwrap();
        assert_eq!(engine.get_current_balance(), dec!(100.0));
    }

    #[test]
    fn test_deposit_zero_fails() {
        let mut engine = BillingEngine::new();
        assert!(engine.deposit(Decimal::ZERO).is_err());
    }

    #[test]
    fn test_deduct_reduces_balance() {
        let mut engine = BillingEngine::new();
        engine.deposit(dec!(50.0)).unwrap();
        engine.deduct(&dec!(20.0)).unwrap();
        assert_eq!(engine.get_current_balance(), dec!(30.0));
    }

    #[test]
    fn test_deduct_insufficient_funds() {
        let mut engine = BillingEngine::new();
        engine.deposit(dec!(10.0)).unwrap();
        let result = engine.deduct(&dec!(20.0));
        assert!(result.is_err());
        assert_eq!(engine.get_current_balance(), dec!(10.0));
    }

    #[test]
    fn test_deduct_to_zero_triggers_freeze() {
        let mut engine = BillingEngine::new();
        engine.deposit(dec!(10.0)).unwrap();
        engine.deduct(&dec!(10.0)).unwrap();
        assert!(engine.is_frozen());
        assert_eq!(engine.get_freeze_state(), FreezeState::FrozenExport);
    }

    #[test]
    fn test_deposit_unfreezes() {
        let mut engine = BillingEngine::new();
        engine.deposit(dec!(10.0)).unwrap();
        engine.deduct(&dec!(10.0)).unwrap();
        assert!(engine.is_frozen());
        engine.deposit(dec!(5.0)).unwrap();
        assert!(!engine.is_frozen());
        assert_eq!(engine.get_freeze_state(), FreezeState::Active);
    }

    #[test]
    fn test_deduct_while_frozen_fails() {
        let mut engine = BillingEngine::new();
        engine.deposit(dec!(10.0)).unwrap();
        engine.deduct(&dec!(10.0)).unwrap();
        assert!(engine.is_frozen());
        // Even if we somehow had balance (shouldn't happen but testing guard)
        let result = engine.deduct(&dec!(1.0));
        assert!(result.is_err());
    }

    #[test]
    fn test_estimate_upload_cost_basic() {
        let engine = BillingEngine::new();
        // 1 TB on HDD, 2x replication, no cushion
        // 0.30 * 1.0 * 2 = 0.60 RUB/hour
        let cost = engine.estimate_upload_cost(rates::BYTES_PER_TB, 2, "hdd", false).unwrap();
        assert_eq!(cost, dec!(0.60));
    }

    #[test]
    fn test_estimate_upload_cost_with_cushion() {
        let engine = BillingEngine::new();
        // 1 TB on SSD, 3x replication, with cushion
        // 0.30 * 1.5 * 3 = 1.35; * 1.25 = 1.6875
        let cost = engine.estimate_upload_cost(rates::BYTES_PER_TB, 3, "ssd", true).unwrap();
        assert_eq!(cost, dec!(1.6875));
    }

    #[test]
    fn test_estimate_upload_cost_small_file() {
        let engine = BillingEngine::new();
        // 1 GB on HDD, 2x, no cushion
        // 0.30 * 1.0 * 2 = 0.60 per TB/hour
        // 1 GB = 0.001 TB -> 0.60 * 0.001 = 0.0006
        let cost = engine.estimate_upload_cost(1_000_000_000, 2, "hdd", false).unwrap();
        assert_eq!(cost, dec!(0.00060000));
    }

    #[test]
    fn test_estimate_upload_cost_invalid_disk() {
        let engine = BillingEngine::new();
        let result = engine.estimate_upload_cost(1_000_000, 2, "floppy", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_estimate_upload_cost_invalid_replication() {
        let engine = BillingEngine::new();
        let result = engine.estimate_upload_cost(1_000_000, 1, "hdd", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_subscription_upgrade() {
        let mut engine = BillingEngine::with_tier(SubscriptionTier::Archive);
        let net = engine.set_subscription(SubscriptionTier::Standard).unwrap();
        assert!(net > Decimal::ZERO);
        assert_eq!(engine.get_subscription(), SubscriptionTier::Standard);
    }

    #[test]
    fn test_set_subscription_downgrade() {
        let mut engine = BillingEngine::with_tier(SubscriptionTier::Premium);
        let net = engine.set_subscription(SubscriptionTier::Standard).unwrap();
        assert!(net < Decimal::ZERO);
        assert_eq!(engine.get_subscription(), SubscriptionTier::Standard);
    }

    #[test]
    fn test_remaining_export_seconds_active() {
        let engine = BillingEngine::new();
        assert_eq!(engine.remaining_export_seconds(), 0);
    }

    #[test]
    fn test_remaining_export_seconds_frozen() {
        let mut engine = BillingEngine::new();
        engine.deposit(dec!(10.0)).unwrap();
        engine.deduct(&dec!(10.0)).unwrap();
        let remaining = engine.remaining_export_seconds();
        assert!(remaining > 172700); // close to 48h
        assert!(remaining <= 172800);
    }

    #[test]
    fn test_full_lifecycle() {
        let mut engine = BillingEngine::new();

        // 1. Start with zero
        assert_eq!(engine.get_current_balance(), Decimal::ZERO);

        // 2. Deposit
        engine.deposit(dec!(1000.0)).unwrap();
        assert_eq!(engine.get_current_balance(), dec!(1000.0));

        // 3. Subscribe
        let net = engine.set_subscription(SubscriptionTier::Premium).unwrap();
        assert!(net > Decimal::ZERO);

        // 4. Estimate cost
        let cost = engine.estimate_upload_cost(500_000_000_000, 3, "ssd", true).unwrap();
        assert!(cost > Decimal::ZERO);

        // 5. Deduct
        engine.deduct(&dec!(500.0)).unwrap();
        assert_eq!(engine.get_current_balance(), dec!(500.0));

        // 6. Drain balance to trigger freeze
        engine.deduct(&dec!(500.0)).unwrap();
        assert!(engine.is_frozen());
        assert_eq!(engine.get_freeze_state(), FreezeState::FrozenExport);

        // 7. Deposit to unfreeze
        engine.deposit(dec!(100.0)).unwrap();
        assert!(!engine.is_frozen());
    }
}
