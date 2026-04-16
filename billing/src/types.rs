use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[cfg(test)]
use rust_decimal_macros::dec;

use crate::rates::split_revenue;

/// Unique account identifier.
pub type AccountId = uuid::Uuid;

/// Types of financial transactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionType {
    Deposit,
    Charge,
    Refund,
    Subscription,
    FreezePenalty,
}

/// A single financial transaction record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: uuid::Uuid,
    pub account_id: AccountId,
    pub tx_type: TransactionType,
    pub amount: Decimal,
    pub description: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Transaction {
    pub fn new(account_id: AccountId, tx_type: TransactionType, amount: Decimal, description: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            account_id,
            tx_type,
            amount,
            description,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// A billing entry representing a charge with platform/host split.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingEntry {
    pub id: uuid::Uuid,
    pub account_id: AccountId,
    pub total_amount: Decimal,
    pub platform_fee: Decimal,
    pub host_fee: Decimal,
    pub description: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl BillingEntry {
    /// Creates a new billing entry, computing the platform/host fee split.
    pub fn new(account_id: AccountId, total_amount: Decimal, description: String) -> Self {
        let (platform_fee, host_fee) = split_revenue(total_amount);
        Self {
            id: uuid::Uuid::new_v4(),
            account_id,
            total_amount,
            platform_fee,
            host_fee,
            description,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Verifies that platform_fee + host_fee equals total_amount.
    pub fn verify_split(&self) -> bool {
        self.platform_fee + self.host_fee == self.total_amount
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_billing_entry_split_round() {
        let entry = BillingEntry::new(AccountId::nil(), dec!(100.0), "test".into());
        assert_eq!(entry.platform_fee, dec!(10.0));
        assert_eq!(entry.host_fee, dec!(90.0));
        assert!(entry.verify_split());
    }

    #[test]
    fn test_billing_entry_split_fraction() {
        let entry = BillingEntry::new(AccountId::nil(), dec!(0.33), "fraction test".into());
        assert!(entry.verify_split());
        assert_eq!(entry.platform_fee + entry.host_fee, dec!(0.33));
    }

    #[test]
    fn test_billing_entry_verify_split_always_true() {
        for val in [dec!(1.0), dec!(0.01), dec!(999999.999), dec!(0.001)] {
            let entry = BillingEntry::new(AccountId::nil(), val, "verify".into());
            assert!(entry.verify_split(), "split must be exact for {}", val);
        }
    }

    #[test]
    fn test_transaction_creation() {
        let tx = Transaction::new(
            AccountId::nil(),
            TransactionType::Deposit,
            dec!(50.0),
            "top-up".into(),
        );
        assert_eq!(tx.tx_type, TransactionType::Deposit);
        assert_eq!(tx.amount, dec!(50.0));
    }
}
