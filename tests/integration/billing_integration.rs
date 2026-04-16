#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    #[test]
    fn test_billing_full_lifecycle() {
        let mut engine = vaultkeeper_billing::BillingEngine::new();
        engine.deposit(dec!(500.00)).unwrap();
        assert_eq!(engine.get_current_balance(), dec!(500.00));

        let cost = engine.estimate_upload_cost(1_099_511_627_776, 3, "ssd", false).unwrap();
        engine.deduct(&cost).unwrap();
        assert!(engine.get_current_balance() < dec!(500.00));
    }

    #[test]
    fn test_freeze_on_zero_balance() {
        let mut engine = vaultkeeper_billing::BillingEngine::new();
        engine.deposit(dec!(1.00)).unwrap();
        engine.deduct(&dec!(1.00)).unwrap();
        assert!(engine.is_frozen());
    }

    #[test]
    fn test_subscription_pro_rata() {
        let mut engine = vaultkeeper_billing::BillingEngine::new();
        engine.set_subscription(vaultkeeper_billing::SubscriptionTier::Standard).unwrap();
        let net = engine.set_subscription(vaultkeeper_billing::SubscriptionTier::Premium).unwrap();
        assert!(net > dec!(0));
    }

    #[test]
    fn test_90_10_split_no_loss() {
        let (platform, host) = vaultkeeper_billing::rates::split_revenue(dec!(1.00));
        assert_eq!(platform + host, dec!(1.00));
    }
}
