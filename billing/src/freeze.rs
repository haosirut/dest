use serde::{Deserialize, Serialize};

/// Duration of the export window after freeze, in seconds (48 hours).
pub const EXPORT_WINDOW_SECONDS: i64 = 48 * 3600;

/// Possible states of a frozen account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FreezeState {
    /// Account is active and operational.
    Active,
    /// Account is frozen; user has a limited time to export data.
    FrozenExport,
    /// Export window expired; data is marked for hard deletion.
    HardDeleted,
}

/// Manages the freeze lifecycle for an account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreezeManager {
    state: FreezeState,
    /// Timestamp (chrono::DateTime<Utc>) when the freeze was triggered.
    frozen_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl FreezeManager {
    /// Creates a new FreezeManager in the Active state.
    pub fn new() -> Self {
        Self {
            state: FreezeState::Active,
            frozen_at: None,
        }
    }

    /// Returns the current freeze state.
    pub fn state(&self) -> FreezeState {
        self.state
    }

    /// Triggers a freeze. If already frozen, this is a no-op.
    pub fn trigger_freeze(&mut self) {
        if self.state == FreezeState::Active {
            self.state = FreezeState::FrozenExport;
            self.frozen_at = Some(chrono::Utc::now());
            tracing::info!("Account frozen at {:?}", self.frozen_at);
        }
    }

    /// Unfreezes an account back to Active state, clearing the frozen timestamp.
    pub fn unfreeze(&mut self) {
        self.state = FreezeState::Active;
        self.frozen_at = None;
        tracing::info!("Account unfrozen");
    }

    /// Checks whether the export window has expired and transitions to HardDeleted if so.
    pub fn check_export_expiry(&mut self) {
        if self.state != FreezeState::FrozenExport {
            return;
        }
        if self.remaining_export_seconds() <= 0 {
            self.state = FreezeState::HardDeleted;
            tracing::warn!("Export window expired — account hard-deleted");
        }
    }

    /// Returns the number of seconds remaining in the export window.
    /// Returns 0 if not frozen or if the window has expired.
    pub fn remaining_export_seconds(&self) -> i64 {
        match (self.state, self.frozen_at) {
            (FreezeState::FrozenExport, Some(frozen_at)) => {
                let deadline = frozen_at + chrono::Duration::seconds(EXPORT_WINDOW_SECONDS);
                let now = chrono::Utc::now();
                let remaining = deadline.signed_duration_since(now).num_seconds();
                if remaining < 0 {
                    0
                } else {
                    remaining
                }
            }
            _ => 0,
        }
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
    fn test_new_is_active() {
        let fm = FreezeManager::new();
        assert_eq!(fm.state(), FreezeState::Active);
    }

    #[test]
    fn test_trigger_freeze_transitions() {
        let mut fm = FreezeManager::new();
        fm.trigger_freeze();
        assert_eq!(fm.state(), FreezeState::FrozenExport);
    }

    #[test]
    fn test_double_freeze_is_noop() {
        let mut fm = FreezeManager::new();
        fm.trigger_freeze();
        let first_frozen_at = fm.frozen_at;
        fm.trigger_freeze();
        assert_eq!(fm.state(), FreezeState::FrozenExport);
        assert_eq!(fm.frozen_at, first_frozen_at);
    }

    #[test]
    fn test_unfreeze_from_frozen() {
        let mut fm = FreezeManager::new();
        fm.trigger_freeze();
        fm.unfreeze();
        assert_eq!(fm.state(), FreezeState::Active);
        assert!(fm.frozen_at.is_none());
    }

    #[test]
    fn test_unfreeze_from_active_is_noop() {
        let mut fm = FreezeManager::new();
        fm.unfreeze();
        assert_eq!(fm.state(), FreezeState::Active);
    }

    #[test]
    fn test_remaining_seconds_when_active() {
        let fm = FreezeManager::new();
        assert_eq!(fm.remaining_export_seconds(), 0);
    }

    #[test]
    fn test_remaining_seconds_after_freeze() {
        let mut fm = FreezeManager::new();
        fm.trigger_freeze();
        let remaining = fm.remaining_export_seconds();
        // Should be very close to EXPORT_WINDOW_SECONDS (48h = 172800s)
        assert!(remaining > EXPORT_WINDOW_SECONDS - 10);
        assert!(remaining <= EXPORT_WINDOW_SECONDS);
    }

    #[test]
    fn test_check_expiry_not_yet() {
        let mut fm = FreezeManager::new();
        fm.trigger_freeze();
        fm.check_export_expiry();
        assert_eq!(fm.state(), FreezeState::FrozenExport);
    }

    #[test]
    fn test_check_expiry_expired() {
        let mut fm = FreezeManager::new();
        // Manually set frozen_at to well in the past
        fm.frozen_at = Some(chrono::Utc::now() - chrono::Duration::seconds(EXPORT_WINDOW_SECONDS + 1));
        fm.state = FreezeState::FrozenExport;
        fm.check_export_expiry();
        assert_eq!(fm.state(), FreezeState::HardDeleted);
    }

    #[test]
    fn test_unfreeze_from_hard_deleted() {
        let mut fm = FreezeManager::new();
        fm.frozen_at = Some(chrono::Utc::now() - chrono::Duration::seconds(EXPORT_WINDOW_SECONDS + 1));
        fm.state = FreezeState::HardDeleted;
        fm.unfreeze();
        assert_eq!(fm.state(), FreezeState::Active);
    }
}
