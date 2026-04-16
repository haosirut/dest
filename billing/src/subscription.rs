//! Subscription tiers and management.
//!
//! Archive (0 RUB):   weight=1, standard queue, basic SLA
//! Standard (199 RUB): weight=2, priority queue, enhanced SLA
//! Premium (499 RUB):  weight=3, critical priority, best SLA, extra replication

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Subscription tier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionTier {
    Archive,
    Standard,
    Premium,
}

impl SubscriptionTier {
    pub fn name(&self) -> &'static str {
        match self {
            SubscriptionTier::Archive => "archive",
            SubscriptionTier::Standard => "standard",
            SubscriptionTier::Premium => "premium",
        }
    }

    pub fn price_monthly(&self) -> Decimal {
        match self {
            SubscriptionTier::Archive => dec!(0.00),
            SubscriptionTier::Standard => dec!(199.00),
            SubscriptionTier::Premium => dec!(499.00),
        }
    }

    pub fn weight(&self) -> u8 {
        match self {
            SubscriptionTier::Archive => 1,
            SubscriptionTier::Standard => 2,
            SubscriptionTier::Premium => 3,
        }
    }

    /// Rate limit: max upload MB/s
    pub fn upload_rate_limit(&self) -> u32 {
        match self {
            SubscriptionTier::Archive => 10,
            SubscriptionTier::Standard => 50,
            SubscriptionTier::Premium => 0, // unlimited
        }
    }

    /// Rate limit: max download MB/s
    pub fn download_rate_limit(&self) -> u32 {
        match self {
            SubscriptionTier::Archive => 20,
            SubscriptionTier::Standard => 100,
            SubscriptionTier::Premium => 0, // unlimited
        }
    }

    /// SLA: guaranteed uptime percentage
    pub fn sla_uptime(&self) -> Decimal {
        match self {
            SubscriptionTier::Archive => dec!(99.0),
            SubscriptionTier::Standard => dec!(99.9),
            SubscriptionTier::Premium => dec!(99.99),
        }
    }

    /// Max simultaneous replication threads
    pub fn max_replication_threads(&self) -> u32 {
        match self {
            SubscriptionTier::Archive => 1,
            SubscriptionTier::Standard => 3,
            SubscriptionTier::Premium => 8,
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "standard" => SubscriptionTier::Standard,
            "premium" => SubscriptionTier::Premium,
            _ => SubscriptionTier::Archive,
        }
    }
}

/// Subscription state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionState {
    pub tier: SubscriptionTier,
    pub started_at: DateTime<Utc>,
    pub paid_until: DateTime<Utc>,
    pub auto_renew: bool,
}

impl SubscriptionState {
    pub fn is_active(&self) -> bool {
        Utc::now() < self.paid_until
    }

    pub fn days_remaining(&self) -> i64 {
        (self.paid_until - Utc::now()).num_days().max(0)
    }
}

/// Subscription manager
pub struct SubscriptionManager {
    pub current: SubscriptionState,
}

impl SubscriptionManager {
    /// Create new subscription manager with a free archive tier
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            current: SubscriptionState {
                tier: SubscriptionTier::Archive,
                started_at: now,
                paid_until: now,
                auto_renew: false,
            },
        }
    }

    /// Subscribe to a paid tier
    pub fn subscribe(&mut self, tier: SubscriptionTier) -> anyhow::Result<()> {
        let price = tier.price_monthly();
        if price > Decimal::ZERO {
            // In production, this would check account balance first
            info!("Subscribing to {} tier at {} RUB/month", tier.name(), price);
        }

        let now = Utc::now();
        let paid_until = now + chrono::Duration::days(30);

        self.current = SubscriptionState {
            tier,
            started_at: now,
            paid_until,
            auto_renew: true,
        };

        info!("Subscription activated: {} (until {})", tier.name(), paid_until);
        Ok(())
    }

    /// Change subscription tier with pro-rata calculation
    pub fn change_tier(&mut self, new_tier: SubscriptionTier) -> anyhow::Result<Decimal> {
        let old_tier = self.current.tier;
        if old_tier == new_tier {
            anyhow::bail!("Already on {} tier", new_tier.name());
        }

        let remaining_days = self.current.days_remaining() as u32;
        let used_days = 30 - remaining_days;

        // Pro-rata refund for unused days of old tier
        let old_daily = old_tier.price_monthly() / dec!(30);
        let refund = old_daily * Decimal::from(remaining_days);

        // Pro-rata charge for remaining days on new tier
        let new_daily = new_tier.price_monthly() / dec!(30);
        let charge = new_daily * Decimal::from(remaining_days);

        // Net amount: charge - refund
        let net_amount = charge - refund;

        let now = Utc::now();
        self.current = SubscriptionState {
            tier: new_tier,
            started_at: self.current.started_at,
            paid_until: now + chrono::Duration::days(remaining_days as i64),
            auto_renew: true,
        };

        info!(
            "Tier changed: {} -> {}, pro-rata: refund {} + charge {} = net {}",
            old_tier.name(), new_tier.name(), refund, charge, net_amount
        );

        Ok(net_amount)
    }

    /// Get current subscription tier
    pub fn current_tier(&self) -> SubscriptionTier {
        self.current.tier
    }

    /// Check if subscription is active
    pub fn is_active(&self) -> bool {
        self.current.is_active()
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_tier_archive() {
        let manager = SubscriptionManager::new();
        assert_eq!(manager.current_tier(), SubscriptionTier::Archive);
        assert_eq!(manager.current_tier().price_monthly(), dec!(0.00));
    }

    #[test]
    fn test_subscribe_standard() {
        let mut manager = SubscriptionManager::new();
        manager.subscribe(SubscriptionTier::Standard).unwrap();
        assert_eq!(manager.current_tier(), SubscriptionTier::Standard);
        assert!(manager.is_active());
    }

    #[test]
    fn test_subscribe_premium() {
        let mut manager = SubscriptionManager::new();
        manager.subscribe(SubscriptionTier::Premium).unwrap();
        assert_eq!(manager.current_tier(), SubscriptionTier::Premium);
        assert_eq!(manager.current_tier().weight(), 3);
    }

    #[test]
    fn test_tier_prices() {
        assert_eq!(SubscriptionTier::Archive.price_monthly(), dec!(0.00));
        assert_eq!(SubscriptionTier::Standard.price_monthly(), dec!(199.00));
        assert_eq!(SubscriptionTier::Premium.price_monthly(), dec!(499.00));
    }

    #[test]
    fn test_tier_weights() {
        assert_eq!(SubscriptionTier::Archive.weight(), 1);
        assert_eq!(SubscriptionTier::Standard.weight(), 2);
        assert_eq!(SubscriptionTier::Premium.weight(), 3);
    }

    #[test]
    fn test_rate_limits() {
        assert!(SubscriptionTier::Archive.upload_rate_limit() < SubscriptionTier::Standard.upload_rate_limit());
        assert!(SubscriptionTier::Premium.upload_rate_limit() == 0); // unlimited
    }

    #[test]
    fn test_change_tier_pro_rata() {
        let mut manager = SubscriptionManager::new();
        manager.subscribe(SubscriptionTier::Standard).unwrap();
        let net = manager.change_tier(SubscriptionTier::Premium).unwrap();
        assert_eq!(manager.current_tier(), SubscriptionTier::Premium);
        // Premium is more expensive, so net should be positive
        assert!(net > Decimal::ZERO);
    }

    #[test]
    fn test_change_tier_same_tier_fails() {
        let mut manager = SubscriptionManager::new();
        manager.subscribe(SubscriptionTier::Standard).unwrap();
        let result = manager.change_tier(SubscriptionTier::Standard);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_name() {
        assert_eq!(SubscriptionTier::from_name("archive"), SubscriptionTier::Archive);
        assert_eq!(SubscriptionTier::from_name("standard"), SubscriptionTier::Standard);
        assert_eq!(SubscriptionTier::from_name("premium"), SubscriptionTier::Premium);
        assert_eq!(SubscriptionTier::from_name("unknown"), SubscriptionTier::Archive);
    }
}
