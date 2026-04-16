use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Base rate: 0.30 RUB per TB per hour.
pub const BASE_RATE: Decimal = dec!(0.30);

/// Platform share of revenue: 10%.
pub const PLATFORM_SHARE: Decimal = dec!(0.10);

/// Host share of revenue: 90%.
pub const HOST_SHARE: Decimal = dec!(0.90);

/// Cushion multiplier applied when cushion pricing is enabled.
pub const CUSHION_MULTIPLIER: Decimal = dec!(1.25);

/// Number of bytes in one terabyte for cost calculations.
pub const BYTES_PER_TB: u64 = 1_000_000_000_000;

/// Disk type classification with associated cost multiplier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskType {
    Hdd,
    Ssd,
    Nvme,
}

impl DiskType {
    /// Returns the cost multiplier for this disk type.
    /// HDD = 1.0, SSD = 1.5, NVMe = 2.0
    pub fn multiplier(&self) -> Decimal {
        match self {
            DiskType::Hdd => dec!(1.0),
            DiskType::Ssd => dec!(1.5),
            DiskType::Nvme => dec!(2.0),
        }
    }
}

impl FromStr for DiskType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hdd" => Ok(DiskType::Hdd),
            "ssd" => Ok(DiskType::Ssd),
            "nvme" => Ok(DiskType::Nvme),
            _ => anyhow::bail!("Unknown disk type: '{}'. Expected: hdd, ssd, nvme", s),
        }
    }
}

impl std::fmt::Display for DiskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiskType::Hdd => write!(f, "HDD"),
            DiskType::Ssd => write!(f, "SSD"),
            DiskType::Nvme => write!(f, "NVMe"),
        }
    }
}

/// Calculates the hourly rate for a given disk type, replication factor, and cushion setting.
///
/// Formula: `BASE_RATE * disk_multiplier * replication_factor * cushion_multiplier`
pub fn calculate_hourly_rate(
    disk: DiskType,
    replication_factor: u8,
    cushion: bool,
) -> Decimal {
    let replication = Decimal::from(replication_factor);
    let rate = BASE_RATE * disk.multiplier() * replication;
    if cushion {
        rate * CUSHION_MULTIPLIER
    } else {
        rate
    }
}

/// Splits a total revenue amount into platform and host portions.
///
/// The host portion is calculated as `total - platform` to avoid rounding loss
/// so that `platform + host == total` is always exact.
pub fn split_revenue(total: Decimal) -> (Decimal, Decimal) {
    let platform = total * PLATFORM_SHARE;
    // Round platform to 4 decimal places for consistency
    let platform = platform.round_dp(4);
    let host = total - platform;
    (platform, host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_rate_is_correct() {
        assert_eq!(BASE_RATE, dec!(0.30));
    }

    #[test]
    fn test_disk_type_hdd_multiplier() {
        assert_eq!(DiskType::Hdd.multiplier(), dec!(1.0));
    }

    #[test]
    fn test_disk_type_ssd_multiplier() {
        assert_eq!(DiskType::Ssd.multiplier(), dec!(1.5));
    }

    #[test]
    fn test_disk_type_nvme_multiplier() {
        assert_eq!(DiskType::Nvme.multiplier(), dec!(2.0));
    }

    #[test]
    fn test_hourly_rate_hdd_no_replication_no_cushion() {
        let rate = calculate_hourly_rate(DiskType::Hdd, 1, false);
        assert_eq!(rate, dec!(0.30));
    }

    #[test]
    fn test_hourly_rate_ssd_2x_replication() {
        let rate = calculate_hourly_rate(DiskType::Ssd, 2, false);
        // 0.30 * 1.5 * 2 = 0.90
        assert_eq!(rate, dec!(0.90));
    }

    #[test]
    fn test_hourly_rate_nvme_3x_replication_with_cushion() {
        let rate = calculate_hourly_rate(DiskType::Nvme, 3, true);
        // 0.30 * 2.0 * 3 = 1.80; * 1.25 = 2.25
        assert_eq!(rate, dec!(2.25));
    }

    #[test]
    fn test_cushion_adds_25_percent() {
        let without = calculate_hourly_rate(DiskType::Hdd, 2, false);
        let with = calculate_hourly_rate(DiskType::Hdd, 2, true);
        // without = 0.30 * 1.0 * 2 = 0.60
        assert_eq!(without, dec!(0.60));
        // with = 0.60 * 1.25 = 0.75
        assert_eq!(with, dec!(0.75));
    }

    #[test]
    fn test_split_revenue_round_numbers() {
        let (platform, host) = split_revenue(dec!(100.0));
        assert_eq!(platform, dec!(10.0));
        assert_eq!(host, dec!(90.0));
        assert_eq!(platform + host, dec!(100.0));
    }

    #[test]
    fn test_split_revenue_tricky_fraction() {
        let (platform, host) = split_revenue(dec!(0.33));
        assert_eq!(platform + host, dec!(0.33));
        // platform = 0.33 * 0.10 = 0.033 -> rounded to 0.0330
        assert_eq!(platform, dec!(0.0330));
        assert_eq!(host, dec!(0.2970));
    }

    #[test]
    fn test_split_revenue_always_equals_total() {
        // Test several tricky values to verify no rounding loss
        for val in [dec!(1.0), dec!(0.01), dec!(99.99), dec!(0.001), dec!(12345.6789)] {
            let (platform, host) = split_revenue(val);
            assert_eq!(
                platform + host,
                val,
                "split_revenue({}) should sum back to itself: {} + {} = {}",
                val,
                platform,
                host,
                platform + host
            );
        }
    }

    #[test]
    fn test_disk_type_from_str() {
        assert_eq!(DiskType::from_str("hdd").unwrap(), DiskType::Hdd);
        assert_eq!(DiskType::from_str("SSD").unwrap(), DiskType::Ssd);
        assert_eq!(DiskType::from_str("NvMe").unwrap(), DiskType::Nvme);
        assert!(DiskType::from_str("floppy").is_err());
    }

    #[test]
    fn test_replication_4x() {
        let rate = calculate_hourly_rate(DiskType::Ssd, 4, false);
        // 0.30 * 1.5 * 4 = 1.80
        assert_eq!(rate, dec!(1.80));
    }
}
