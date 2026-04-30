//! Billing types and data structures.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
#[cfg(test)]
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique billing account identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub String);

impl AccountId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for AccountId {
    fn default() -> Self {
        Self::new()
    }
}

/// A single billing entry (one hour of usage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingEntry {
    pub id: String,
    pub account_id: AccountId,
    pub timestamp: DateTime<Utc>,
    pub hours: u32,
    pub data_volume_tb: Decimal,
    pub hourly_rate: Decimal,
    pub total_cost: Decimal,
    pub platform_fee: Decimal,
    pub host_fee: Decimal,
    pub disk_type: u32,
    pub replication_factor: u32,
    pub cushion_enabled: bool,
}

impl BillingEntry {
    pub fn new(
        account_id: AccountId,
        timestamp: DateTime<Utc>,
        hours: u32,
        data_volume_tb: Decimal,
        hourly_rate: Decimal,
        disk_type: u32,
        replication_factor: u32,
        cushion_enabled: bool,
    ) -> Self {
        let total_cost = hourly_rate * data_volume_tb * Decimal::from(hours);
        let platform_fee = total_cost * crate::rates::PLATFORM_SHARE;
        let host_fee = total_cost * crate::rates::HOST_SHARE;

        Self {
            id: Uuid::new_v4().to_string(),
            account_id,
            timestamp,
            hours,
            data_volume_tb,
            hourly_rate,
            total_cost,
            platform_fee,
            host_fee,
            disk_type,
            replication_factor,
            cushion_enabled,
        }
    }

    /// Verify that platform_fee + host_fee == total_cost (no rounding loss)
    pub fn verify_split(&self) -> bool {
        self.platform_fee + self.host_fee == self.total_cost
    }
}

/// Transaction types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    BillingCharge,
    Refund,
    PlatformFee,
    HostPayout,
}

/// A financial transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub account_id: AccountId,
    pub tx_type: TransactionType,
    pub amount: Decimal,
    pub balance_after: Decimal,
    pub timestamp: DateTime<Utc>,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_billing_entry_split_no_loss() {
        let entry = BillingEntry::new(
            AccountId::new(),
            Utc::now(),
            1,
            dec!(1.0),
            dec!(0.30),
            1,
            2,
            false,
        );
        assert!(entry.verify_split());
        assert_eq!(entry.platform_fee + entry.host_fee, entry.total_cost);
    }

    #[test]
    fn test_billing_entry_calculation() {
        let entry = BillingEntry::new(
            AccountId::new(),
            Utc::now(),
            1,
            dec!(1.0),
            dec!(0.60),
            2,
            2,
            false,
        );
        assert_eq!(entry.total_cost, dec!(0.60));
        assert_eq!(entry.platform_fee, dec!(0.06));
        assert_eq!(entry.host_fee, dec!(0.54));
    }

    #[test]
    fn test_account_id_unique() {
        let id1 = AccountId::new();
        let id2 = AccountId::new();
        assert_ne!(id1, id2);
    }
}
