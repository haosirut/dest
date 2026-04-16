//! Billing Aggregator — aggregates charges per host, not per chunk.
//!
//! Problem: Without aggregation, 1 TB of data at 4 MB chunks = 262,144 individual
//! charges. With aggregation, this becomes 50–200 monthly payments per host.
//!
//! Payout thresholds:
//!   - Balance >= 100 RUB → payout
//!   - OR 7 days since last payout → payout
//!
//! Uses `rust_decimal` with ROUND_HALF_UP for tax-compatible precision.

use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Rate: 0.30 RUB per TB per hour.
pub const HOURLY_RATE_PER_TB: Decimal = Decimal::new(30, 2);

/// Minimum balance (RUB) to trigger automatic payout.
pub const PAYOUT_THRESHOLD_RUB: Decimal = Decimal::new(100, 0);

/// Maximum days between payouts (even if below threshold).
pub const MAX_DAYS_BETWEEN_PAYOUTS: u32 = 7;

/// Multiplier for HDD storage (cheapest).
pub const DISK_MULTIPLIER_HDD: Decimal = Decimal::new(80, 2); // 0.80

/// Multiplier for SSD storage (baseline).
pub const DISK_MULTIPLIER_SSD: Decimal = Decimal::new(100, 2); // 1.00

/// Multiplier for NVMe storage (premium).
pub const DISK_MULTIPLIER_NVME: Decimal = Decimal::new(150, 2); // 1.50

/// Platform commission percentage (10%).
pub const PLATFORM_COMMISSION_PCT: Decimal = Decimal::new(10, 1); // 10.0

/// Host payout percentage (90%).
pub const HOST_PAYOUT_PCT: Decimal = Decimal::new(90, 1); // 90.0

/// A single chunk stored by a host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredChunk {
    /// Chunk identifier (blake3 hex).
    pub chunk_id: String,
    /// Host peer ID storing this chunk.
    pub host_id: String,
    /// Chunk size in bytes.
    pub size_bytes: u64,
    /// Disk type used by the host.
    pub disk_type: String,
    /// Hours this chunk has been stored.
    pub hours_stored: u64,
}

/// Aggregated billing record for a single host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostBillingRecord {
    /// Host peer ID.
    pub host_id: String,
    /// Total number of chunks hosted.
    pub total_chunks: u32,
    /// Total weighted hours (hours * size * disk_multiplier).
    pub weighted_hours: Decimal,
    /// Calculated gross earnings (before commission).
    pub gross_earnings: Decimal,
    /// Platform commission (10%).
    pub platform_commission: Decimal,
    /// Host payout (90%).
    pub host_payout: Decimal,
    /// Days since last payout.
    pub days_since_last_payout: u32,
}

/// Result of processing hourly billing for all active chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingCycleResult {
    /// Total hosts billed in this cycle.
    pub hosts_billed: usize,
    /// Total gross earnings across all hosts.
    pub total_gross: Decimal,
    /// Total platform commission collected.
    pub total_commission: Decimal,
    /// Total host payouts due.
    pub total_payouts: Decimal,
    /// Individual host records.
    pub records: Vec<HostBillingRecord>,
}

/// Calculate the disk type multiplier.
pub fn disk_multiplier(disk_type: &str) -> Decimal {
    match disk_type.to_lowercase().as_str() {
        "hdd" => DISK_MULTIPLIER_HDD,
        "nvme" => DISK_MULTIPLIER_NVME,
        _ => DISK_MULTIPLIER_SSD, // default to SSD
    }
}

/// Aggregate billing for active chunks grouped by host.
///
/// 1. Groups chunks by host_id
/// 2. Calculates weighted hours for each chunk (size_TB * hours * disk_multiplier)
/// 3. Computes gross earnings = weighted_hours * HOURLY_RATE_PER_TB
/// 4. Splits: 90% host, 10% platform
/// 5. Flags hosts eligible for payout (>=100 RUB or >=7 days)
pub fn aggregate_host_billing(
    active_chunks: &[StoredChunk],
    days_since_last_payout: &HashMap<String, u32>,
) -> BillingCycleResult {
    let mut host_data: HashMap<String, HostAccumulator> = HashMap::new();

    for chunk in active_chunks {
        let entry = host_data.entry(chunk.host_id.clone()).or_default();
        entry.total_chunks += 1;

        // size in TB (decimal, exact)
        let size_tb = Decimal::from(chunk.size_bytes)
            .checked_div(Decimal::from(1_099_511_627_776u64)) // 1 TB in bytes
            .unwrap_or(Decimal::ZERO);

        let multiplier = disk_multiplier(&chunk.disk_type);
        let hours = Decimal::from(chunk.hours_stored);

        // Weighted hours = size_TB * hours * disk_multiplier
        let weighted = (size_tb * hours * multiplier).round_dp_with_strategy(
            10,
            RoundingStrategy::MidpointAwayFromZero,
        );
        entry.weighted_hours += weighted;
    }

    let mut records: Vec<HostBillingRecord> = Vec::new();
    let mut total_gross = Decimal::ZERO;
    let mut total_commission = Decimal::ZERO;
    let mut total_payouts = Decimal::ZERO;

    for (host_id, acc) in host_data {
        let gross = (acc.weighted_hours * HOURLY_RATE_PER_TB).round_dp_with_strategy(
            2,
            RoundingStrategy::MidpointAwayFromZero,
        );

        let commission = (gross * PLATFORM_COMMISSION_PCT / Decimal::from(100))
            .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
        let payout = (gross - commission).round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);

        let days = days_since_last_payout
            .get(&host_id)
            .copied()
            .unwrap_or(MAX_DAYS_BETWEEN_PAYOUTS); // default: eligible

        total_gross += gross;
        total_commission += commission;
        total_payouts += payout;

        records.push(HostBillingRecord {
            host_id,
            total_chunks: acc.total_chunks,
            weighted_hours: acc.weighted_hours,
            gross_earnings: gross,
            platform_commission: commission,
            host_payout: payout,
            days_since_last_payout: days,
        });
    }

    BillingCycleResult {
        hosts_billed: records.len(),
        total_gross,
        total_commission,
        total_payouts,
        records,
    }
}

/// Check if a host record is eligible for payout.
///
/// A host is eligible if:
/// - Their accumulated balance >= 100 RUB, OR
/// - Days since last payout >= 7
pub fn is_payout_eligible(record: &HostBillingRecord) -> bool {
    record.host_payout >= PAYOUT_THRESHOLD_RUB
        || record.days_since_last_payout >= MAX_DAYS_BETWEEN_PAYOUTS
}

/// Process hourly billing cycle and return only hosts eligible for payout.
pub fn process_hourly_billing(
    active_chunks: Vec<StoredChunk>,
    days_since_last_payout: &HashMap<String, u32>,
) -> Result<Vec<HostBillingRecord>, String> {
    let result = aggregate_host_billing(&active_chunks, days_since_last_payout);

    let eligible: Vec<HostBillingRecord> = result
        .records
        .into_iter()
        .filter(|r| is_payout_eligible(r))
        .collect();

    Ok(eligible)
}

// ─── Internal accumulator ──────────────────────────────────────────────────

#[derive(Default)]
struct HostAccumulator {
    total_chunks: u32,
    weighted_hours: Decimal,
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_multiplier() {
        assert_eq!(disk_multiplier("hdd"), DISK_MULTIPLIER_HDD);
        assert_eq!(disk_multiplier("ssd"), DISK_MULTIPLIER_SSD);
        assert_eq!(disk_multiplier("nvme"), DISK_MULTIPLIER_NVME);
        assert_eq!(disk_multiplier("unknown"), DISK_MULTIPLIER_SSD);
    }

    #[test]
    fn test_aggregate_empty() {
        let result = aggregate_host_billing(&[], &HashMap::new());
        assert_eq!(result.hosts_billed, 0);
        assert_eq!(result.total_gross, Decimal::ZERO);
    }

    #[test]
    fn test_aggregate_single_host() {
        let chunks = vec![StoredChunk {
            chunk_id: "abc".into(),
            host_id: "host1".into(),
            size_bytes: 4 * 1024 * 1024, // 4 MB
            disk_type: "ssd".into(),
            hours_stored: 1,
        }];

        let result = aggregate_host_billing(&chunks, &HashMap::new());
        assert_eq!(result.hosts_billed, 1);
        assert_eq!(result.records[0].total_chunks, 1);
    }

    #[test]
    fn test_payout_eligible_by_amount() {
        // Manually create a record above threshold
        let record = HostBillingRecord {
            host_id: "rich_host".into(),
            total_chunks: 1000,
            weighted_hours: Decimal::new(1000, 0),
            gross_earnings: Decimal::new(300, 0), // 300 RUB gross
            platform_commission: Decimal::new(30, 0), // 30 RUB commission
            host_payout: Decimal::new(270, 0), // 270 RUB payout
            days_since_last_payout: 1,
        };
        assert!(is_payout_eligible(&record));
    }

    #[test]
    fn test_payout_eligible_by_days() {
        let record = HostBillingRecord {
            host_id: "patient_host".into(),
            total_chunks: 10,
            weighted_hours: Decimal::new(10, 0),
            gross_earnings: Decimal::new(3, 0), // 3 RUB gross
            platform_commission: Decimal::new(30, 1), // 0.3 RUB commission
            host_payout: Decimal::new(270, 2), // 2.70 RUB payout
            days_since_last_payout: 10, // >= 7 days
        };
        assert!(is_payout_eligible(&record));
    }

    #[test]
    fn test_payout_not_eligible() {
        let record = HostBillingRecord {
            host_id: "new_host".into(),
            total_chunks: 1,
            weighted_hours: Decimal::new(1, 0),
            gross_earnings: Decimal::new(1, 0),
            platform_commission: Decimal::new(10, 1),
            host_payout: Decimal::new(90, 1), // 0.90 RUB
            days_since_last_payout: 1,
        };
        assert!(!is_payout_eligible(&record));
    }

    #[test]
    fn test_commission_split() {
        // 100 RUB gross → 10 RUB platform, 90 RUB host
        let gross = Decimal::new(100, 0);
        let commission = (gross * PLATFORM_COMMISSION_PCT / Decimal::from(100))
            .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
        let payout = gross - commission;
        assert_eq!(commission, Decimal::new(10, 0));
        assert_eq!(payout, Decimal::new(90, 0));
    }

    #[test]
    fn test_process_hourly_billing() {
        let chunks = vec![StoredChunk {
            chunk_id: "chunk1".into(),
            host_id: "host1".into(),
            size_bytes: 100 * 1024 * 1024 * 1024, // ~100 GB
            disk_type: "ssd".into(),
            hours_stored: 720, // 30 days
        }];

        let mut days = HashMap::new();
        days.insert("host1".to_string(), 10); // 10 days since last payout

        let result = process_hourly_billing(chunks, &days).unwrap();
        // Should be eligible because days >= 7
        assert!(!result.is_empty());
    }

    #[test]
    fn test_multiple_hosts_aggregation() {
        let chunks = vec![
            StoredChunk {
                chunk_id: "c1".into(), host_id: "h1".into(),
                size_bytes: 4 * 1024 * 1024, disk_type: "ssd".into(), hours_stored: 1,
            },
            StoredChunk {
                chunk_id: "c2".into(), host_id: "h1".into(),
                size_bytes: 4 * 1024 * 1024, disk_type: "ssd".into(), hours_stored: 1,
            },
            StoredChunk {
                chunk_id: "c3".into(), host_id: "h2".into(),
                size_bytes: 4 * 1024 * 1024, disk_type: "hdd".into(), hours_stored: 2,
            },
        ];

        let result = aggregate_host_billing(&chunks, &HashMap::new());
        assert_eq!(result.hosts_billed, 2);
    }
}
