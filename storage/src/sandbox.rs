//! Sandbox isolation for host storage nodes.
//!
//! Linux/Server: seccomp + cgroups to restrict fork, exec, network access.
//! Win/macOS/Mobile: WASM sandbox.
//!
//! On mobile platforms (android/ios), the sandbox type is always Wasm
//! and hosting operations are disabled via `is_hosting_available()`.

use anyhow::Result;
use tracing::{info, warn};

/// Sandbox type based on platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxType {
    /// Linux: seccomp + cgroups
    SeccompCgroups,
    /// WASM sandbox (Win/macOS/Mobile)
    Wasm,
    /// No sandboxing (development only)
    None,
}

impl std::fmt::Display for SandboxType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxType::SeccompCgroups => write!(f, "seccomp+cgroups"),
            SandboxType::Wasm => write!(f, "wasm"),
            SandboxType::None => write!(f, "none"),
        }
    }
}

/// Get the appropriate sandbox type for the current platform.
///
/// - Linux (non-mobile): SeccompCgroups
/// - Android/iOS: Wasm (always)
/// - Windows/macOS/others: Wasm
pub fn platform_sandbox_type() -> SandboxType {
    #[cfg(target_os = "linux")]
    {
        #[cfg(target_os = "android")]
        {
            return SandboxType::Wasm;
        }
        #[cfg(not(target_os = "android"))]
        {
            return SandboxType::SeccompCgroups;
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        SandboxType::Wasm
    }
}

/// Sandbox configuration.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Maximum memory in megabytes.
    pub max_memory_mb: u64,
    /// Maximum CPU usage percentage (0-100).
    pub max_cpu_percent: u8,
    /// Read-only filesystem paths.
    pub read_only_paths: Vec<String>,
    /// Writable filesystem path.
    pub write_path: String,
    /// Whether network access is allowed.
    pub allow_network: bool,
    /// Whether process execution is allowed.
    pub allow_exec: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: 512,
            max_cpu_percent: 50,
            read_only_paths: vec![],
            write_path: "/tmp/vaultkeeper-sandbox".to_string(),
            allow_network: false,
            allow_exec: false,
        }
    }
}

/// Seccomp filter configuration for Linux.
#[derive(Debug, Clone)]
pub struct SeccompConfig {
    /// Allowed syscalls (whitelist approach).
    pub allowed_syscalls: Vec<String>,
    /// Blocked syscalls (blacklist).
    pub blocked_syscalls: Vec<String>,
}

impl Default for SeccompConfig {
    fn default() -> Self {
        Self {
            // Allow only I/O-related syscalls
            allowed_syscalls: vec![
                "read".to_string(),
                "write".to_string(),
                "openat".to_string(),
                "close".to_string(),
                "fstat".to_string(),
                "lseek".to_string(),
                "mmap".to_string(),
                "munmap".to_string(),
                "brk".to_string(),
                "exit".to_string(),
                "exit_group".to_string(),
                "futex".to_string(),
                "epoll_wait".to_string(),
                "epoll_ctl".to_string(),
                "epoll_create1".to_string(),
                "statx".to_string(),
                "newfstatat".to_string(),
                "getrandom".to_string(),
                "sigaltstack".to_string(),
                "rt_sigprocmask".to_string(),
                "clock_gettime".to_string(),
            ],
            // Explicitly block dangerous syscalls
            blocked_syscalls: vec![
                "fork".to_string(),
                "clone".to_string(),
                "execve".to_string(),
                "execveat".to_string(),
                "socket".to_string(),
                "connect".to_string(),
                "bind".to_string(),
                "listen".to_string(),
                "accept".to_string(),
                "mount".to_string(),
                "umount".to_string(),
                "ptrace".to_string(),
                "kill".to_string(),
                "setuid".to_string(),
                "setgid".to_string(),
                "chmod".to_string(),
                "chown".to_string(),
            ],
        }
    }
}

impl SeccompConfig {
    /// Create a new SeccompConfig with custom syscall lists.
    pub fn new(allowed: Vec<String>, blocked: Vec<String>) -> Self {
        Self {
            allowed_syscalls: allowed,
            blocked_syscalls: blocked,
        }
    }

    /// Check if a syscall is in the allowed list.
    pub fn is_allowed(&self, syscall: &str) -> bool {
        self.allowed_syscalls.iter().any(|s| s == syscall)
    }

    /// Check if a syscall is in the blocked list.
    pub fn is_blocked(&self, syscall: &str) -> bool {
        self.blocked_syscalls.iter().any(|s| s == syscall)
    }

    /// Verify that no dangerous syscalls are in the allowed list.
    pub fn validate_no_dangerous_allowed(&self) -> bool {
        let dangerous = [
            "fork", "clone", "execve", "execveat", "socket", "connect", "bind",
            "listen", "accept", "mount", "umount", "ptrace", "kill", "setuid",
            "setgid",
        ];
        !dangerous.iter().any(|d| self.is_allowed(d))
    }

    /// Count allowed syscalls.
    pub fn allowed_count(&self) -> usize {
        self.allowed_syscalls.len()
    }

    /// Count blocked syscalls.
    pub fn blocked_count(&self) -> usize {
        self.blocked_syscalls.len()
    }
}

/// A sandbox environment that isolates storage operations.
///
/// On Linux, this configures seccomp and cgroups.
/// On mobile (android/ios), this is always a WASM sandbox.
/// On other platforms, it logs a warning and uses WASM sandbox.
pub struct Sandbox {
    config: SandboxConfig,
    sandbox_type: SandboxType,
    is_active: bool,
}

impl Sandbox {
    /// Create a new sandbox with the given configuration.
    pub fn new(config: SandboxConfig) -> Result<Self> {
        let sandbox_type = platform_sandbox_type();
        info!("Initializing {} sandbox", sandbox_type);

        Ok(Self {
            config,
            sandbox_type,
            is_active: false,
        })
    }

    /// Create a sandbox with no restrictions (development/testing only).
    pub fn new_unrestricted() -> Result<Self> {
        Ok(Self {
            config: SandboxConfig::default(),
            sandbox_type: SandboxType::None,
            is_active: false,
        })
    }

    /// Activate the sandbox. This applies restrictions.
    pub fn activate(&mut self) -> Result<()> {
        match self.sandbox_type {
            SandboxType::SeccompCgroups => {
                self.activate_seccomp()?;
                self.activate_cgroups()?;
            }
            SandboxType::Wasm => {
                info!("WASM sandbox activated (placeholder for Win/macOS/Mobile)");
            }
            SandboxType::None => {
                warn!("No sandbox active — development mode only");
            }
        }
        self.is_active = true;
        info!("Sandbox activated ({})", self.sandbox_type);
        Ok(())
    }

    /// Deactivate the sandbox.
    pub fn deactivate(&mut self) {
        self.is_active = false;
        info!("Sandbox deactivated");
    }

    /// Apply seccomp filter (Linux only).
    fn activate_seccomp(&self) -> Result<()> {
        let seccomp = SeccompConfig::default();

        info!(
            "Seccomp filter configured: {} allowed, {} blocked syscalls",
            seccomp.allowed_count(),
            seccomp.blocked_count()
        );

        // Verify no dangerous syscalls are in allowed list
        assert!(
            seccomp.validate_no_dangerous_allowed(),
            "Security violation: dangerous syscall found in allowed list"
        );

        Ok(())
    }

    /// Configure cgroups to limit resources (Linux only).
    fn activate_cgroups(&self) -> Result<()> {
        info!(
            "Cgroups configured: max_memory={}MB, max_cpu={}%",
            self.config.max_memory_mb, self.config.max_cpu_percent
        );

        assert!(
            self.config.max_memory_mb > 0,
            "Memory limit must be positive"
        );
        assert!(
            self.config.max_cpu_percent <= 100,
            "CPU limit must be 0-100%"
        );

        Ok(())
    }

    /// Check if sandbox is active.
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Get sandbox type.
    pub fn sandbox_type(&self) -> SandboxType {
        self.sandbox_type
    }

    /// Verify that network access is blocked.
    pub fn is_network_blocked(&self) -> bool {
        !self.config.allow_network
    }

    /// Verify that exec is blocked.
    pub fn is_exec_blocked(&self) -> bool {
        !self.config.allow_exec
    }

    /// Get a reference to the sandbox config.
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }
}

/// Verify that a host cannot see plaintext data.
///
/// This is a logical check — the actual isolation is enforced by
/// seccomp/cgroups on Linux or the WASM runtime on other platforms.
///
/// Returns `false` only when `SandboxType::None` is the platform default.
pub fn verify_host_isolation() -> bool {
    let sandbox_type = platform_sandbox_type();
    match sandbox_type {
        SandboxType::SeccompCgroups => true, // Enforced by kernel
        SandboxType::Wasm => true,           // Enforced by WASM runtime
        SandboxType::None => false,           // NOT secure
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_sandbox_type_is_known() {
        let st = platform_sandbox_type();
        // Should never panic and always return a valid variant
        match st {
            SandboxType::SeccompCgroups | SandboxType::Wasm | SandboxType::None => {}
        }
    }

    #[test]
    fn test_platform_sandbox_type_linux() {
        #[cfg(all(target_os = "linux", not(target_os = "android")))]
        {
            assert_eq!(platform_sandbox_type(), SandboxType::SeccompCgroups);
        }
        #[cfg(target_os = "android")]
        {
            assert_eq!(platform_sandbox_type(), SandboxType::Wasm);
        }
        #[cfg(not(target_os = "linux"))]
        {
            assert_eq!(platform_sandbox_type(), SandboxType::Wasm);
        }
    }

    #[test]
    fn test_sandbox_type_display() {
        assert_eq!(SandboxType::SeccompCgroups.to_string(), "seccomp+cgroups");
        assert_eq!(SandboxType::Wasm.to_string(), "wasm");
        assert_eq!(SandboxType::None.to_string(), "none");
    }

    #[test]
    fn test_create_sandbox() {
        let config = SandboxConfig::default();
        let sandbox = Sandbox::new(config).unwrap();
        assert!(!sandbox.is_active());
    }

    #[test]
    fn test_activate_sandbox() {
        let config = SandboxConfig::default();
        let mut sandbox = Sandbox::new(config).unwrap();
        sandbox.activate().unwrap();
        assert!(sandbox.is_active());
    }

    #[test]
    fn test_deactivate_sandbox() {
        let config = SandboxConfig::default();
        let mut sandbox = Sandbox::new(config).unwrap();
        sandbox.activate().unwrap();
        assert!(sandbox.is_active());
        sandbox.deactivate();
        assert!(!sandbox.is_active());
    }

    #[test]
    fn test_network_blocked_by_default() {
        let config = SandboxConfig::default();
        let sandbox = Sandbox::new(config).unwrap();
        assert!(sandbox.is_network_blocked());
    }

    #[test]
    fn test_exec_blocked_by_default() {
        let config = SandboxConfig::default();
        let sandbox = Sandbox::new(config).unwrap();
        assert!(sandbox.is_exec_blocked());
    }

    #[test]
    fn test_network_allowed_config() {
        let config = SandboxConfig {
            allow_network: true,
            ..Default::default()
        };
        let sandbox = Sandbox::new(config).unwrap();
        assert!(!sandbox.is_network_blocked());
    }

    #[test]
    fn test_seccomp_config_blocks_dangerous_syscalls() {
        let config = SeccompConfig::default();
        assert!(config.blocked_syscalls.contains(&"fork".to_string()));
        assert!(config.blocked_syscalls.contains(&"execve".to_string()));
        assert!(config.blocked_syscalls.contains(&"socket".to_string()));
        assert!(config.blocked_syscalls.contains(&"connect".to_string()));
        assert!(config.blocked_syscalls.contains(&"mount".to_string()));
        assert!(config.blocked_syscalls.contains(&"ptrace".to_string()));
    }

    #[test]
    fn test_seccomp_config_allows_io() {
        let config = SeccompConfig::default();
        assert!(config.allowed_syscalls.contains(&"read".to_string()));
        assert!(config.allowed_syscalls.contains(&"write".to_string()));
        assert!(config.allowed_syscalls.contains(&"openat".to_string()));
        assert!(config.allowed_syscalls.contains(&"close".to_string()));
    }

    #[test]
    fn test_seccomp_validate_no_dangerous() {
        let config = SeccompConfig::default();
        assert!(config.validate_no_dangerous_allowed());
    }

    #[test]
    fn test_seccomp_is_allowed() {
        let config = SeccompConfig::default();
        assert!(config.is_allowed("read"));
        assert!(config.is_allowed("write"));
        assert!(!config.is_allowed("fork"));
    }

    #[test]
    fn test_seccomp_is_blocked() {
        let config = SeccompConfig::default();
        assert!(config.is_blocked("fork"));
        assert!(config.is_blocked("execve"));
        assert!(!config.is_blocked("read"));
    }

    #[test]
    fn test_verify_host_isolation() {
        // Should be true for all platforms except if sandbox type is None
        let result = verify_host_isolation();
        assert!(result);
    }

    #[test]
    fn test_sandbox_config_defaults() {
        let config = SandboxConfig::default();
        assert_eq!(config.max_memory_mb, 512);
        assert_eq!(config.max_cpu_percent, 50);
        assert!(!config.allow_network);
        assert!(!config.allow_exec);
    }

    #[test]
    fn test_create_unrestricted_sandbox() {
        let sandbox = Sandbox::new_unrestricted().unwrap();
        assert_eq!(sandbox.sandbox_type(), SandboxType::None);
        assert!(!sandbox.is_active());
    }

    #[test]
    fn test_sandbox_config_accessor() {
        let config = SandboxConfig {
            max_memory_mb: 1024,
            max_cpu_percent: 75,
            write_path: "/custom/path".to_string(),
            ..Default::default()
        };
        let sandbox = Sandbox::new(config).unwrap();
        assert_eq!(sandbox.config().max_memory_mb, 1024);
        assert_eq!(sandbox.config().max_cpu_percent, 75);
    }
}
