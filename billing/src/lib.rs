//! VaultKeeper Billing — Hourly billing calculations with Decimal precision.
//!
//! Tariff: 0.30 RUB/TB/hour base
//! Distribution: 10% platform, 90% hosts
//! Freeze: balance <= 0 triggers instant freeze -> 48h export window -> hard delete
//! Cushion: +25% to tariff when enabled

pub mod account;
pub mod calculator;
pub mod freeze;
pub mod rates;
pub mod subscription;
pub mod types;

pub use account::BillingAccount;
pub use calculator::BillingCalculator;
pub use freeze::{FreezeManager, FreezeState};
pub use subscription::{SubscriptionTier, SubscriptionManager};
