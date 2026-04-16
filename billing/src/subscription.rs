use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Subscription tiers with associated pricing and limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum SubscriptionTier {
    Archive,
    Standard,
    Premium,
}

impl SubscriptionTier {
    /// Monthly price in RUB.
    pub fn price_monthly(&self) -> Decimal {
        match self {
            SubscriptionTier::Archive => dec!(0),
            SubscriptionTier::Standard => dec!(199),
            SubscriptionTier::Premium => dec!(499),
        }
    }

    /// Daily pro-rata price (monthly / 30).
    pub fn price_daily(&self) -> Decimal {
        self.price_monthly() / dec!(30)
    }

    /// Priority weight for resource allocation (higher = more priority).
    pub fn weight(&self) -> u32 {
        match self {
            SubscriptionTier::Archive => 1,
            SubscriptionTier::Standard => 5,
            SubscriptionTier::Premium => 10,
        }
    }

    /// Maximum upload rate in MB/s.
    pub fn rate_limits(&self) -> u32 {
        match self {
            SubscriptionTier::Archive => 10,
            SubscriptionTier::Standard => 50,
            SubscriptionTier::Premium => 200,
        }
    }

    /// Guaranteed SLA uptime percentage (e.g., 99.5 means 99.5%).
    pub fn sla_uptime(&self) -> Decimal {
        match self {
            SubscriptionTier::Archive => dec!(99.0),
            SubscriptionTier::Standard => dec!(99.5),
            SubscriptionTier::Premium => dec!(99.9),
        }
    }

    /// Maximum number of concurrent replication threads.
    pub fn max_replication_threads(&self) -> u8 {
        match self {
            SubscriptionTier::Archive => 1,
            SubscriptionTier::Standard => 3,
            SubscriptionTier::Premium => 8,
        }
    }

    /// Display name for the tier.
    pub fn name(&self) -> &'static str {
        match self {
            SubscriptionTier::Archive => "Archive",
            SubscriptionTier::Standard => "Standard",
            SubscriptionTier::Premium => "Premium",
        }
    }
}

impl FromStr for SubscriptionTier {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "archive" => Ok(SubscriptionTier::Archive),
            "standard" => Ok(SubscriptionTier::Standard),
            "premium" => Ok(SubscriptionTier::Premium),
            _ => anyhow::bail!("Unknown subscription tier: '{}'. Expected: archive, standard, premium", s),
        }
    }
}

impl std::fmt::Display for SubscriptionTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Manages a user's subscription state and tier changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionManager {
    current_tier: SubscriptionTier,
    /// When the current billing period started.
    period_start: chrono::DateTime<chrono::Utc>,
    /// Seconds in a billing period (30 days).
    period_seconds: i64,
}

impl SubscriptionManager {
    /// Creates a new subscription manager starting with the given tier.
    pub fn new(tier: SubscriptionTier) -> Self {
        Self {
            current_tier: tier,
            period_start: chrono::Utc::now(),
            period_seconds: 30 * 24 * 3600,
        }
    }

    /// Returns the current subscription tier.
    pub fn current_tier(&self) -> SubscriptionTier {
        self.current_tier
    }

    /// Parses a tier name string (case-insensitive).
    pub fn from_name(name: &str) -> Result<SubscriptionTier, anyhow::Error> {
        SubscriptionTier::from_str(name)
    }

    /// Subscribes to a new tier (no previous tier). Returns the full monthly price as a debit.
    pub fn subscribe(&mut self, tier: SubscriptionTier) -> Decimal {
        let price = tier.price_monthly();
        self.current_tier = tier;
        self.period_start = chrono::Utc::now();
        price
    }

    /// Changes from the current tier to a new tier with pro-rata calculation.
    ///
    /// Formula:
    /// - daily_old = old_tier.price_monthly / 30
    /// - daily_new = new_tier.price_monthly / 30
    /// - days_remaining = (period_end - now).days (ceiling)
    /// - old_remaining = daily_old * days_remaining
    /// - new_remaining = daily_new * days_remaining
    /// - net = new_remaining - old_remaining
    ///
    /// Returns the pro-rata net amount (positive = charge, negative = credit).
    pub fn change_tier(&mut self, new_tier: SubscriptionTier) -> Decimal {
        let now = chrono::Utc::now();
        let period_end = self.period_start + chrono::Duration::seconds(self.period_seconds);
        let remaining_duration = period_end.signed_duration_since(now);
        let days_remaining = if remaining_duration.num_seconds() > 0 {
            // Ceiling division: (seconds + seconds_per_day - 1) / seconds_per_day
            let secs = remaining_duration.num_seconds();
            (secs + 86399) / 86400
        } else {
            0
        };

        let old_daily = self.current_tier.price_daily();
        let new_daily = new_tier.price_daily();

        let old_remaining = old_daily * Decimal::from(days_remaining);
        let new_remaining = new_daily * Decimal::from(days_remaining);

        let net = new_remaining - old_remaining;

        self.current_tier = new_tier;
        self.period_start = chrono::Utc::now();

        net
    }

    /// Returns the fraction of the billing period that has elapsed (0.0 to 1.0).
    pub fn period_progress(&self) -> Decimal {
        let now = chrono::Utc::now();
        let elapsed = now.signed_duration_since(self.period_start).num_seconds();
        let progress = Decimal::from(elapsed) / Decimal::from(self.period_seconds);
        if progress < dec!(0) {
            dec!(0)
        } else if progress > dec!(1) {
            dec!(1)
        } else {
            progress
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_archive_price() {
        assert_eq!(SubscriptionTier::Archive.price_monthly(), dec!(0));
    }

    #[test]
    fn test_standard_price() {
        assert_eq!(SubscriptionTier::Standard.price_monthly(), dec!(199));
    }

    #[test]
    fn test_premium_price() {
        assert_eq!(SubscriptionTier::Premium.price_monthly(), dec!(499));
    }

    #[test]
    fn test_daily_price() {
        // 199 / 30 = 6.6333...
        let daily = SubscriptionTier::Standard.price_daily();
        assert_eq!(daily, dec!(199) / dec!(30));
    }

    #[test]
    fn test_tier_weights() {
        assert!(SubscriptionTier::Archive.weight() < SubscriptionTier::Standard.weight());
        assert!(SubscriptionTier::Standard.weight() < SubscriptionTier::Premium.weight());
    }

    #[test]
    fn test_rate_limits() {
        assert_eq!(SubscriptionTier::Archive.rate_limits(), 10);
        assert_eq!(SubscriptionTier::Standard.rate_limits(), 50);
        assert_eq!(SubscriptionTier::Premium.rate_limits(), 200);
    }

    #[test]
    fn test_sla_uptime() {
        assert!(SubscriptionTier::Archive.sla_uptime() < SubscriptionTier::Standard.sla_uptime());
        assert!(SubscriptionTier::Standard.sla_uptime() < SubscriptionTier::Premium.sla_uptime());
    }

    #[test]
    fn test_max_replication_threads() {
        assert_eq!(SubscriptionTier::Archive.max_replication_threads(), 1);
        assert_eq!(SubscriptionTier::Standard.max_replication_threads(), 3);
        assert_eq!(SubscriptionTier::Premium.max_replication_threads(), 8);
    }

    #[test]
    fn test_from_name_case_insensitive() {
        use std::str::FromStr;
        assert_eq!(SubscriptionTier::from_str("ARCHIVE").unwrap(), SubscriptionTier::Archive);
        assert_eq!(SubscriptionTier::from_str("Standard").unwrap(), SubscriptionTier::Standard);
        assert!(SubscriptionTier::from_str("enterprise").is_err());
    }

    #[test]
    fn test_subscribe_sets_tier() {
        let mut sm = SubscriptionManager::new(SubscriptionTier::Archive);
        let price = sm.subscribe(SubscriptionTier::Standard);
        assert_eq!(sm.current_tier(), SubscriptionTier::Standard);
        assert_eq!(price, dec!(199));
    }

    #[test]
    fn test_change_tier_upgrade() {
        let mut sm = SubscriptionManager::new(SubscriptionTier::Archive);
        let net = sm.change_tier(SubscriptionTier::Standard);
        // Upgrading from free to paid: net should be positive
        assert!(net > dec!(0));
        assert_eq!(sm.current_tier(), SubscriptionTier::Standard);
    }

    #[test]
    fn test_change_tier_downgrade() {
        let mut sm = SubscriptionManager::new(SubscriptionTier::Premium);
        let net = sm.change_tier(SubscriptionTier::Standard);
        // Downgrading: net should be negative (credit)
        assert!(net < dec!(0));
        assert_eq!(sm.current_tier(), SubscriptionTier::Standard);
    }

    #[test]
    fn test_change_tier_same_tier() {
        let mut sm = SubscriptionManager::new(SubscriptionTier::Standard);
        let net = sm.change_tier(SubscriptionTier::Standard);
        // Same tier: net should be 0
        assert_eq!(net, dec!(0));
    }

    #[test]
    fn test_period_progress() {
        let sm = SubscriptionManager::new(SubscriptionTier::Standard);
        let progress = sm.period_progress();
        // Just created, so progress should be very close to 0
        assert!(progress >= dec!(0));
        assert!(progress < dec!(0.01));
    }
}
