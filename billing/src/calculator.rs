//! Billing calculator — computes hourly charges with Decimal precision.

use crate::rates::*;
use crate::types::BillingEntry;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// The main billing calculator
pub struct BillingCalculator;

impl BillingCalculator {
    /// Calculate a single hourly billing entry.
    /// data_volume_bytes: stored bytes on disk
    /// disk_type: HDD=1, SSD=2, NVMe=3
    /// replication: number of replicas
    /// cushion: +25% to rate
    pub fn calculate_hourly(
        account_id: crate::types::AccountId,
        data_volume_bytes: u64,
        disk_type: DiskType,
        replication: ReplicationFactor,
        cushion_enabled: bool,
    ) -> BillingEntry {
        let data_volume_tb = Decimal::from(data_volume_bytes)
            / dec!(1_099_511_627_776); // 1 TB = 2^40 bytes

        let hourly_rate = calculate_hourly_rate(disk_type, replication, cushion_enabled);

        BillingEntry::new(
            account_id,
            chrono::Utc::now(),
            1,
            data_volume_tb,
            hourly_rate,
            disk_type as u32,
            replication.0,
            cushion_enabled,
        )
    }

    /// Split a total cost between platform and hosts.
    /// Returns (platform_amount, host_amount).
    /// Guaranteed: platform + host == total (no rounding loss).
    pub fn split_revenue(total: Decimal) -> (Decimal, Decimal) {
        let platform = total * PLATFORM_SHARE;
        let host = total - platform; // Avoids rounding: host = 90% exactly
        (platform, host)
    }

    /// Verify that split has no rounding loss.
    pub fn verify_split(total: Decimal, platform: Decimal, host: Decimal) -> bool {
        total == platform + host
    }

    /// Calculate daily cost from hourly rate and data volume.
    pub fn daily_cost(hourly_rate: Decimal, data_volume_tb: Decimal) -> Decimal {
        hourly_rate * data_volume_tb * HOURS_PER_DAY
    }

    /// Calculate monthly cost (30 days) from daily cost.
    pub fn monthly_cost(daily_cost: Decimal) -> Decimal {
        daily_cost * dec!(30)
    }

    /// Check if balance is sufficient for continued service.
    /// Returns true if balance > 0 (prepaid model).
    pub fn is_balance_sufficient(balance: Decimal) -> bool {
        balance > Decimal::ZERO
    }

    /// Calculate remaining service hours at current spend rate.
    pub fn remaining_hours(balance: Decimal, hourly_rate: Decimal, data_volume_tb: Decimal) -> Decimal {
        let hourly_cost = hourly_rate * data_volume_tb;
        if hourly_cost == Decimal::ZERO {
            return Decimal::MAX;
        }
        balance / hourly_cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AccountId;

    #[test]
    fn test_calculate_hourly_hdd_1tb() {
        let entry = BillingCalculator::calculate_hourly(
            AccountId::new(),
            1_099_511_627_776, // exactly 1 TB
            DiskType::Hdd,
            ReplicationFactor(1),
            false,
        );
        assert_eq!(entry.hourly_rate, dec!(0.30));
    }

    #[test]
    fn test_calculate_hourly_ssd_2x() {
        let entry = BillingCalculator::calculate_hourly(
            AccountId::new(),
            1_099_511_627_776,
            DiskType::Ssd,
            ReplicationFactor(2),
            false,
        );
        assert_eq!(entry.hourly_rate, dec!(0.90));
    }

    #[test]
    fn test_split_revenue_no_rounding() {
        let total = dec!(1.00);
        let (platform, host) = BillingCalculator::split_revenue(total);
        assert_eq!(platform, dec!(0.10));
        assert_eq!(host, dec!(0.90));
        assert!(BillingCalculator::verify_split(total, platform, host));
    }

    #[test]
    fn test_split_revenue_various_amounts() {
        for i in 1..=100u32 {
            let total = Decimal::from(i) / dec!(7.00); // Tricky fractions
            let (platform, host) = BillingCalculator::split_revenue(total);
            assert!(
                BillingCalculator::verify_split(total, platform, host),
                "Split failed for total: {}", total
            );
        }
    }

    #[test]
    fn test_daily_and_monthly_cost() {
        let hourly = dec!(0.30);
        let tb = dec!(1.0);
        let daily = BillingCalculator::daily_cost(hourly, tb);
        assert_eq!(daily, dec!(7.20)); // 0.30 * 24
        let monthly = BillingCalculator::monthly_cost(daily);
        assert_eq!(monthly, dec!(216.00)); // 7.20 * 30
    }

    #[test]
    fn test_balance_sufficient() {
        assert!(BillingCalculator::is_balance_sufficient(dec!(0.01)));
        assert!(!BillingCalculator::is_balance_sufficient(dec!(0.00)));
        assert!(!BillingCalculator::is_balance_sufficient(dec!(-1.00)));
    }

    #[test]
    fn test_remaining_hours() {
        let balance = dec!(10.00);
        let rate = dec!(0.30);
        let tb = dec!(1.0);
        let hours = BillingCalculator::remaining_hours(balance, rate, tb);
        assert_eq!(hours, dec!(33.33333333333333333333333333));
    }

    #[test]
    fn test_entry_verify_split() {
        let entry = BillingCalculator::calculate_hourly(
            AccountId::new(),
            1_099_511_627_776,
            DiskType::Nvme,
            ReplicationFactor(3),
            true,
        );
        assert!(entry.verify_split());
    }
}
