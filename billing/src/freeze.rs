//! Freeze manager — handles account lifecycle: Active -> FrozenExport -> HardDeleted.
//!
//! On balance <= 0:
//! 1. Instant freeze
//! 2. 48-hour export window (user can download data)
//! 3. Hard delete (all data permanently removed)

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Freeze states for an account
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FreezeState {
    /// Account is active and operational
    Active,
    /// Balance <= 0, user has 48h to export data
    FrozenExport,
    /// 48h elapsed, all data deleted
    HardDeleted,
}

impl std::fmt::Display for FreezeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FreezeState::Active => write!(f, "Active"),
            FreezeState::FrozenExport => write!(f, "FrozenExport"),
            FreezeState::HardDeleted => write!(f, "HardDeleted"),
        }
    }
}

/// Manages account freeze lifecycle
pub struct FreezeManager {
    state: FreezeState,
    frozen_at: Option<DateTime<Utc>>,
    export_duration: Duration,
}

impl FreezeManager {
    /// Create a new freeze manager with default 48-hour export window
    pub fn new() -> Self {
        Self {
            state: FreezeState::Active,
            frozen_at: None,
            export_duration: Duration::hours(48),
        }
    }

    /// Create with custom export duration
    pub fn with_export_duration(hours: i64) -> Self {
        Self {
            state: FreezeState::Active,
            frozen_at: None,
            export_duration: Duration::hours(hours),
        }
    }

    /// Trigger freeze (balance <= 0 detected)
    pub fn trigger_freeze(&mut self) {
        if self.state == FreezeState::Active {
            self.state = FreezeState::FrozenExport;
            self.frozen_at = Some(Utc::now());
            warn!("Account frozen, export window started: {} hours", self.export_duration.num_hours());
        }
    }

    /// Check if the export window has expired, transition to HardDeleted
    pub fn check_export_expiry(&mut self) -> bool {
        if self.state != FreezeState::FrozenExport {
            return false;
        }

        if let Some(frozen_at) = self.frozen_at {
            if Utc::now() >= frozen_at + self.export_duration {
                self.state = FreezeState::HardDeleted;
                warn!("Export window expired, account hard deleted");
                return true;
            }
        }

        false
    }

    /// Get remaining export time in seconds (0 if not frozen)
    pub fn remaining_export_seconds(&self) -> i64 {
        if self.state != FreezeState::FrozenExport {
            return 0;
        }

        if let Some(frozen_at) = self.frozen_at {
            let expiry = frozen_at + self.export_duration;
            let remaining = (expiry - Utc::now()).num_seconds();
            remaining.max(0)
        } else {
            0
        }
    }

    /// Get current state
    pub fn state(&self) -> FreezeState {
        self.state
    }

    /// Unfreeze account (e.g., after deposit)
    pub fn unfreeze(&mut self) {
        if self.state == FreezeState::FrozenExport {
            self.state = FreezeState::Active;
            self.frozen_at = None;
            info!("Account unfrozen after deposit");
        }
    }

    /// Get when the account was frozen
    pub fn frozen_at(&self) -> Option<DateTime<Utc>> {
        self.frozen_at
    }
}

impl Default for FreezeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_active() {
        let manager = FreezeManager::new();
        assert_eq!(manager.state(), FreezeState::Active);
    }

    #[test]
    fn test_trigger_freeze() {
        let mut manager = FreezeManager::new();
        manager.trigger_freeze();
        assert_eq!(manager.state(), FreezeState::FrozenExport);
        assert!(manager.frozen_at().is_some());
    }

    #[test]
    fn test_double_freeze_noop() {
        let mut manager = FreezeManager::new();
        let first_frozen = {
            manager.trigger_freeze();
            manager.frozen_at()
        };
        manager.trigger_freeze();
        assert_eq!(manager.frozen_at(), first_frozen);
    }

    #[test]
    fn test_unfreeze() {
        let mut manager = FreezeManager::new();
        manager.trigger_freeze();
        manager.unfreeze();
        assert_eq!(manager.state(), FreezeState::Active);
        assert!(manager.frozen_at().is_none());
    }

    #[test]
    fn test_unfreeze_hard_deleted_noop() {
        let mut manager = FreezeManager::new();
        manager.trigger_freeze();
        manager.state = FreezeState::HardDeleted;
        manager.unfreeze();
        assert_eq!(manager.state(), FreezeState::HardDeleted);
    }

    #[test]
    fn test_export_expiry_check_not_expired() {
        let mut manager = FreezeManager::with_export_duration(48);
        manager.trigger_freeze();
        assert!(!manager.check_export_expiry());
        assert!(manager.remaining_export_seconds() > 0);
    }

    #[test]
    fn test_export_expiry_after_timeout() {
        let mut manager = FreezeManager::with_export_duration(0); // 0 hours = instant expiry
        manager.trigger_freeze();
        // Wait a tiny bit to ensure time passes
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(manager.check_export_expiry());
        assert_eq!(manager.state(), FreezeState::HardDeleted);
    }

    #[test]
    fn test_remaining_seconds_active() {
        let manager = FreezeManager::new();
        assert_eq!(manager.remaining_export_seconds(), 0);
    }

    #[test]
    fn test_remaining_seconds_frozen() {
        let mut manager = FreezeManager::with_export_duration(48);
        manager.trigger_freeze();
        let remaining = manager.remaining_export_seconds();
        assert!(remaining > 47 * 3600); // At least 47 hours
        assert!(remaining <= 48 * 3600); // At most 48 hours
    }

    #[test]
    fn test_state_display() {
        assert_eq!(FreezeState::Active.to_string(), "Active");
        assert_eq!(FreezeState::FrozenExport.to_string(), "FrozenExport");
        assert_eq!(FreezeState::HardDeleted.to_string(), "HardDeleted");
    }
}
