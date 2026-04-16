//! Tariff rates and multipliers for billing calculations.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// Base rate: 0.30 RUB per TB per hour
pub const BASE_RATE_PER_TB_HOUR: Decimal = dec!(0.30);

/// Platform share: 10%
pub const PLATFORM_SHARE: Decimal = dec!(0.10);

/// Host share: 90%
pub const HOST_SHARE: Decimal = dec!(0.90);

/// Cushion multiplier: +25%
pub const CUSHION_MULTIPLIER: Decimal = dec!(1.25);

/// Hours in a billing period
pub const HOURS_PER_DAY: Decimal = dec!(24);

/// Minimum withdrawal amount in RUB
pub const MIN_WITHDRAWAL: Decimal = dec!(100.00);

/// Disk type multiplier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskType {
    /// HDD: multiplier = 1.0
    Hdd = 1,
    /// SSD: multiplier = 1.5
    Ssd = 2,
    /// NVMe: multiplier = 2.0
    Nvme = 3,
}

impl DiskType {
    pub fn multiplier(&self) -> Decimal {
        match self {
            DiskType::Hdd => dec!(1.0),
            DiskType::Ssd => dec!(1.5),
            DiskType::Nvme => dec!(2.0),
        }
    }

    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => DiskType::Hdd,
            2 => DiskType::Ssd,
            3 => DiskType::Nvme,
            _ => DiskType::Hdd,
        }
    }
}

/// Replication factor multiplier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicationFactor(pub u32);

impl ReplicationFactor {
    pub fn multiplier(&self) -> Decimal {
        Decimal::from(self.0)
    }
}

/// Calculate the hourly cost for given parameters.
/// Formula: base_rate * replication_multiplier * disk_multiplier * (1.25 if cushion)
pub fn calculate_hourly_rate(
    disk_type: DiskType,
    replication: ReplicationFactor,
    cushion_enabled: bool,
) -> Decimal {
    let rate = BASE_RATE_PER_TB_HOUR * replication.multiplier() * disk_type.multiplier();
    if cushion_enabled {
        rate * CUSHION_MULTIPLIER
    } else {
        rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_rate_hdd_no_replication() {
        let rate = calculate_hourly_rate(DiskType::Hdd, ReplicationFactor(1), false);
        assert_eq!(rate, dec!(0.30));
    }

    #[test]
    fn test_ssd_2x_replication() {
        let rate = calculate_hourly_rate(DiskType::Ssd, ReplicationFactor(2), false);
        assert_eq!(rate, dec!(0.90)); // 0.30 * 2 * 1.5
    }

    #[test]
    fn test_nvme_3x_replication_with_cushion() {
        let rate = calculate_hourly_rate(DiskType::Nvme, ReplicationFactor(3), true);
        // 0.30 * 3 * 2.0 * 1.25 = 2.25
        assert_eq!(rate, dec!(2.25));
    }

    #[test]
    fn test_hdd_4x_replication() {
        let rate = calculate_hourly_rate(DiskType::Hdd, ReplicationFactor(4), false);
        assert_eq!(rate, dec!(1.20)); // 0.30 * 4 * 1.0
    }

    #[test]
    fn test_hdd_2x_with_cushion() {
        let rate = calculate_hourly_rate(DiskType::Hdd, ReplicationFactor(2), true);
        // 0.30 * 2 * 1.0 * 1.25 = 0.75
        assert_eq!(rate, dec!(0.75));
    }

    #[test]
    fn test_platform_host_split_sums_to_100() {
        let total = PLATFORM_SHARE + HOST_SHARE;
        assert_eq!(total, dec!(1.00));
    }

    #[test]
    fn test_disk_type_from_u32() {
        assert_eq!(DiskType::from_u32(1), DiskType::Hdd);
        assert_eq!(DiskType::from_u32(2), DiskType::Ssd);
        assert_eq!(DiskType::from_u32(3), DiskType::Nvme);
        assert_eq!(DiskType::from_u32(99), DiskType::Hdd);
    }
}
