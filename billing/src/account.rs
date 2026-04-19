//! Billing account — tracks balance, transactions, and freeze state.

use anyhow::Result;
use crate::freeze::{FreezeManager, FreezeState};
use crate::rates::MIN_WITHDRAWAL;
use crate::types::*;
use chrono::Utc;
use rust_decimal::Decimal;
#[cfg(test)]
use rust_decimal_macros::dec;
use std::collections::VecDeque;
use tracing::{info, warn};

/// Billing account with balance tracking
pub struct BillingAccount {
    pub id: AccountId,
    pub balance: Decimal,
    pub transactions: VecDeque<Transaction>,
    pub freeze_manager: FreezeManager,
}

impl BillingAccount {
    /// Create a new billing account with initial deposit.
    pub fn new(initial_deposit: Decimal) -> Self {
        let id = AccountId::new();
        let mut account = Self {
            id: id.clone(),
            balance: Decimal::ZERO,
            transactions: VecDeque::new(),
            freeze_manager: FreezeManager::new(),
        };

        if initial_deposit > Decimal::ZERO {
            account.deposit(initial_deposit, "Initial deposit").unwrap();
        }

        info!("Created billing account {} with balance {}", id.0, account.balance);
        account
    }

    /// Deposit funds into account.
    pub fn deposit(&mut self, amount: Decimal, description: &str) -> Result<()> {
        if amount <= Decimal::ZERO {
            anyhow::bail!("Deposit amount must be positive");
        }
        if self.freeze_manager.state() == FreezeState::HardDeleted {
            anyhow::bail!("Account has been hard deleted");
        }

        self.balance += amount;
        self.record_transaction(TransactionType::Deposit, amount, description);
        info!("Deposited {} RUB, new balance: {}", amount, self.balance);
        Ok(())
    }

    /// Charge account for billing.
    pub fn charge(&mut self, amount: Decimal, description: &str) -> Result<()> {
        if amount <= Decimal::ZERO {
            anyhow::bail!("Charge amount must be positive");
        }

        self.balance -= amount;
        self.record_transaction(TransactionType::BillingCharge, amount, description);

        // Check freeze condition
        if self.balance <= Decimal::ZERO {
            self.freeze_manager.trigger_freeze();
            warn!("Account {} frozen: balance {} <= 0", self.id.0, self.balance);
        }

        info!("Charged {} RUB, new balance: {}", amount, self.balance);
        Ok(())
    }

    /// Request withdrawal. Requires balance >= MIN_WITHDRAWAL.
    pub fn request_withdrawal(&mut self, amount: Decimal, description: &str) -> Result<()> {
        if amount < MIN_WITHDRAWAL {
            anyhow::bail!("Minimum withdrawal is {} RUB", MIN_WITHDRAWAL);
        }
        if amount > self.balance {
            anyhow::bail!("Insufficient balance: {} < {}", self.balance, amount);
        }
        if self.freeze_manager.state() != FreezeState::Active {
            anyhow::bail!("Account is frozen or deleted, cannot withdraw");
        }

        self.balance -= amount;
        self.record_transaction(TransactionType::Withdrawal, amount, description);
        info!("Withdrawal {} RUB, new balance: {}", amount, self.balance);
        Ok(())
    }

    /// Record a transaction
    fn record_transaction(&mut self, tx_type: TransactionType, amount: Decimal, description: &str) {
        let tx = Transaction {
            id: uuid::Uuid::new_v4().to_string(),
            account_id: self.id.clone(),
            tx_type,
            amount,
            balance_after: self.balance,
            timestamp: Utc::now(),
            description: description.to_string(),
        };
        self.transactions.push_back(tx);
    }

    /// Get current freeze state
    pub fn freeze_state(&self) -> FreezeState {
        self.freeze_manager.state()
    }

    /// Check if account can store data
    pub fn can_store(&self) -> bool {
        self.freeze_manager.state() == FreezeState::Active && self.balance > Decimal::ZERO
    }

    /// Check if account can download data (allowed during freeze export window)
    pub fn can_download(&self) -> bool {
        matches!(
            self.freeze_manager.state(),
            FreezeState::Active | FreezeState::FrozenExport
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_account_with_deposit() {
        let account = BillingAccount::new(dec!(100.00));
        assert_eq!(account.balance, dec!(100.00));
        assert!(account.can_store());
    }

    #[test]
    fn test_deposit() {
        let mut account = BillingAccount::new(dec!(0.00));
        account.deposit(dec!(50.00), "top up").unwrap();
        assert_eq!(account.balance, dec!(50.00));
    }

    #[test]
    fn test_charge_reduces_balance() {
        let mut account = BillingAccount::new(dec!(100.00));
        account.charge(dec!(30.00), "hourly billing").unwrap();
        assert_eq!(account.balance, dec!(70.00));
    }

    #[test]
    fn test_charge_triggers_freeze() {
        let mut account = BillingAccount::new(dec!(10.00));
        account.charge(dec!(10.00), "billing").unwrap();
        assert_eq!(account.freeze_state(), FreezeState::FrozenExport);
        assert!(!account.can_store());
        assert!(account.can_download());
    }

    #[test]
    fn test_withdrawal_success() {
        let mut account = BillingAccount::new(dec!(200.00));
        account.request_withdrawal(dec!(100.00), "withdraw").unwrap();
        assert_eq!(account.balance, dec!(100.00));
    }

    #[test]
    fn test_withdrawal_below_minimum() {
        let mut account = BillingAccount::new(dec!(200.00));
        let result = account.request_withdrawal(dec!(50.00), "too small");
        assert!(result.is_err());
    }

    #[test]
    fn test_withdrawal_insufficient_balance() {
        let mut account = BillingAccount::new(dec!(50.00));
        let result = account.request_withdrawal(dec!(100.00), "too much");
        assert!(result.is_err());
    }

    #[test]
    fn test_deposit_negative_fails() {
        let mut account = BillingAccount::new(dec!(100.00));
        let result = account.deposit(dec!(-10.00), "negative");
        assert!(result.is_err());
    }

    #[test]
    fn test_transactions_recorded() {
        let mut account = BillingAccount::new(dec!(100.00));
        account.deposit(dec!(50.00), "top up").unwrap();
        account.charge(dec!(10.00), "billing").unwrap();
        assert_eq!(account.transactions.len(), 3); // initial + deposit + charge
    }
}
