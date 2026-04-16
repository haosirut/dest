//! VaultKeeper Storage — Storage management layer.
//!
//! Features: seccomp/cgroups sandboxing (Linux), WASM sandbox (mobile/desktop),
//! disk I/O management, shard storage, replication logic, auto-repair on node failure,
//! and mobile platform guards.

pub mod disk;
pub mod replication;
pub mod sandbox;
pub mod shard_store;

/// Returns true if hosting is allowed on this platform.
/// On mobile (android, ios), hosting is DISABLED.
pub fn is_hosting_available() -> bool {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        return false;
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        return true;
    }
}

/// Returns the platform type as a string.
pub fn platform_type() -> &'static str {
    #[cfg(target_os = "android")]
    {
        return "android";
    }
    #[cfg(target_os = "ios")]
    {
        return "ios";
    }
    #[cfg(target_os = "windows")]
    {
        return "windows";
    }
    #[cfg(target_os = "macos")]
    {
        return "macos";
    }
    #[cfg(target_os = "linux")]
    {
        return "linux";
    }
    #[cfg(not(any(
        target_os = "android",
        target_os = "ios",
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    )))]
    {
        return "unknown";
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_hosting_available() {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            assert!(!is_hosting_available());
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            assert!(is_hosting_available());
        }
    }

    #[test]
    fn test_platform_type_is_known() {
        let pt = platform_type();
        assert!(!pt.is_empty());
        assert_ne!(pt, "unknown");
    }
}
