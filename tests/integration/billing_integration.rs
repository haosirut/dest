//! Integration tests for billing system

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    #[test]
    fn test_full_billing_lifecycle() {
        use vaultkeeper_billing::{BillingAccount, BillingCalculator, rates::*};

        // Create account with deposit
        let mut account = BillingAccount::new(dec!(500.00));
        assert_eq!(account.balance, dec!(500.00));

        // Calculate hourly charge: 1TB, HDD, 3x replication
        let entry = BillingCalculator::calculate_hourly(
            account.id.clone(),
            1_099_511_627_776, // 1 TB
            DiskType::Hdd,
            ReplicationFactor(3),
            false,
        );

        // Verify rate: 0.30 * 3 * 1.0 = 0.90 RUB/hour
        assert_eq!(entry.hourly_rate, dec!(0.90));

        // Charge for 1 hour
        account.charge(entry.total_cost, "Hourly billing").unwrap();
        assert!(account.balance < dec!(500.00));
        assert!(account.can_store());

        // Verify split integrity
        assert!(entry.verify_split());
        let (platform, host) = BillingCalculator::split_revenue(entry.total_cost);
        assert_eq!(platform + host, entry.total_cost);
    }

    #[test]
    fn test_freeze_lifecycle() {
        use vaultkeeper_billing::{BillingAccount, rates::*};
        use vaultkeeper_billing::freeze::FreezeState;

        let mut account = BillingAccount::new(dec!(5.00));
        
        // Drain balance to trigger freeze
        account.charge(dec!(5.00), "Final charge").unwrap();
        assert_eq!(account.freeze_state(), FreezeState::FrozenExport);
        assert!(!account.can_store());
        assert!(account.can_download());

        // Deposit to unfreeze
        account.deposit(dec!(100.00), "Refill").unwrap();
        // Note: unfreeze needs explicit call in BillingAccount
    }

    #[test]
    fn test_subscription_pro_rata() {
        use vaultkeeper_billing::SubscriptionManager;
        use vaultkeeper_billing::subscription::SubscriptionTier;

        let mut manager = SubscriptionManager::new();
        manager.subscribe(SubscriptionTier::Standard).unwrap();
        
        let net = manager.change_tier(SubscriptionTier::Premium).unwrap();
        assert!(net > Decimal::ZERO); // Premium costs more
        assert_eq!(manager.current_tier(), SubscriptionTier::Premium);
    }

    #[test]
    fn test_daily_monthly_billing_math() {
        use vaultkeeper_billing::BillingCalculator;
        use vaultkeeper_billing::rates::*;
        use rust_decimal_macros::dec;

        // 2TB on SSD, 2x replication, with cushion
        let hourly = calculate_hourly_rate(DiskType::Ssd, ReplicationFactor(2), true);
        // 0.30 * 2 * 1.5 * 1.25 = 1.125
        assert_eq!(hourly, dec!(1.125));

        let daily = BillingCalculator::daily_cost(hourly, dec!(2.0));
        // 1.125 * 24 * 2 = 54.00
        assert_eq!(daily, dec!(54.00));

        let monthly = BillingCalculator::monthly_cost(daily);
        // 54.00 * 30 = 1620.00
        assert_eq!(monthly, dec!(1620.00));
    }
}
