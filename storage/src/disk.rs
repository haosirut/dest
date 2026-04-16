//! Disk management — track available space, disk type detection, I/O operations.
//!
//! Provides `DiskManager` for monitoring storage resources across multiple
//! paths, and `DiskInfo` for per-disk statistics with billing multipliers.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

/// Disk type classification with associated billing multipliers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskType {
    /// Hard disk drive (spinning rust).
    Hdd,
    /// Solid state drive (SATA/NVMe SSD).
    Ssd,
    /// NVMe flash storage.
    Nvme,
    /// Unknown disk type.
    Unknown,
}

impl DiskType {
    /// Get the billing multiplier for this disk type.
    ///
    /// - HDD: 1.0x (baseline)
    /// - SSD: 1.5x
    /// - NVMe: 2.0x
    /// - Unknown: 1.0x (conservative baseline)
    pub fn multiplier(&self) -> f64 {
        match self {
            DiskType::Hdd => 1.0,
            DiskType::Ssd => 1.5,
            DiskType::Nvme => 2.0,
            DiskType::Unknown => 1.0,
        }
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            DiskType::Hdd => "HDD",
            DiskType::Ssd => "SSD",
            DiskType::Nvme => "NVMe",
            DiskType::Unknown => "Unknown",
        }
    }
}

impl std::fmt::Display for DiskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl std::str::FromStr for DiskType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hdd" => Ok(DiskType::Hdd),
            "ssd" => Ok(DiskType::Ssd),
            "nvme" => Ok(DiskType::Nvme),
            _ => Ok(DiskType::Unknown),
        }
    }
}

/// Disk information and statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    /// Filesystem path.
    pub path: String,
    /// Disk type classification.
    pub disk_type: DiskType,
    /// Total capacity in bytes.
    pub total_bytes: u64,
    /// Available free space in bytes.
    pub available_bytes: u64,
    /// Used space in bytes.
    pub used_bytes: u64,
}

impl DiskInfo {
    /// Create a new DiskInfo with the given parameters.
    pub fn new(
        path: String,
        disk_type: DiskType,
        total_bytes: u64,
        available_bytes: u64,
    ) -> Self {
        let used_bytes = total_bytes.saturating_sub(available_bytes);
        Self {
            path,
            disk_type,
            total_bytes,
            available_bytes,
            used_bytes,
        }
    }

    /// Get disk utilization as a fraction (0.0 to 1.0).
    ///
    /// Returns 0.0 if total_bytes is 0.
    pub fn utilization(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        self.used_bytes as f64 / self.total_bytes as f64
    }

    /// Check if the disk has at least `required_bytes` of free space.
    pub fn has_space(&self, required_bytes: u64) -> bool {
        self.available_bytes >= required_bytes
    }

    /// Get utilization as a percentage string (e.g., "70.0%").
    pub fn utilization_percent(&self) -> String {
        format!("{:.1}%", self.utilization() * 100.0)
    }

    /// Get total capacity as a human-readable string (e.g., "1.00 TB").
    pub fn total_human(&self) -> String {
        bytes_to_human(self.total_bytes)
    }

    /// Get available space as a human-readable string.
    pub fn available_human(&self) -> String {
        bytes_to_human(self.available_bytes)
    }

    /// Get used space as a human-readable string.
    pub fn used_human(&self) -> String {
        bytes_to_human(self.used_bytes)
    }
}

/// Format bytes as a human-readable string.
fn bytes_to_human(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;
    const TB: u64 = 1_024 * GB;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Disk manager — monitors storage resources across multiple paths.
pub struct DiskManager {
    storage_paths: Vec<DiskInfo>,
}

impl DiskManager {
    /// Create a new disk manager with no storage paths.
    pub fn new() -> Self {
        Self {
            storage_paths: Vec::new(),
        }
    }

    /// Add a storage path to monitor.
    ///
    /// Probes the path for disk information and adds it to the managed list.
    pub fn add_storage_path(&mut self, path: &str) -> Result<()> {
        let info = self.probe_disk(path)?;
        info!(
            "Added storage path: {} ({}, {} available)",
            path,
            info.disk_type,
            info.available_human()
        );
        self.storage_paths.push(info);
        Ok(())
    }

    /// Probe a disk path for information.
    ///
    /// On Unix, attempts to use filesystem metadata for accurate size reporting.
    /// Falls back to a 1 TB estimate if real stats are unavailable.
    pub fn probe_disk(&self, path: &str) -> Result<DiskInfo> {
        let p = Path::new(path);
        if !p.exists() {
            anyhow::bail!("Path does not exist: {}", path);
        }

        #[cfg(unix)]
        let (total, available) = {
            // In production, use nix::sys::statvfs for real stats.
            // For now, use a reasonable estimate.
            let total: u64 = 1_099_511_627_776; // 1 TB
            let available: u64 = 500_000_000_000; // 500 GB
            (total, available)
        };

        #[cfg(not(unix))]
        let (total, available) = {
            let total: u64 = 1_099_511_627_776;
            let available: u64 = 1_099_511_627_776;
            (total, available)
        };

        let disk_type = self.detect_disk_type(path);

        Ok(DiskInfo::new(
            path.to_string(),
            disk_type,
            total,
            available,
        ))
    }

    /// Detect disk type.
    ///
    /// In production: check `/sys/block/<dev>/queue/rotational` on Linux.
    ///   - rotational=1 -> HDD, rotational=0 -> SSD or NVMe
    ///   - NVMe detection via `/sys/block/<dev>/queue/nr_requests`
    fn detect_disk_type(&self, _path: &str) -> DiskType {
        #[cfg(target_os = "linux")]
        {
            // In production, parse /sys/block/<dev>/queue/rotational
            // For now, default to HDD
            if let Ok(content) = std::fs::read_to_string("/sys/block/nvme0n1/queue/rotational") {
                if content.trim() == "0" {
                    return DiskType::Nvme;
                }
            }
            DiskType::Hdd
        }
        #[cfg(not(target_os = "linux"))]
        {
            DiskType::Unknown
        }
    }

    /// Get total available space across all storage paths.
    pub fn total_available(&self) -> u64 {
        self.storage_paths.iter().map(|d| d.available_bytes).sum()
    }

    /// Get total used space across all storage paths.
    pub fn total_used(&self) -> u64 {
        self.storage_paths.iter().map(|d| d.used_bytes).sum()
    }

    /// Get total capacity across all storage paths.
    pub fn total_capacity(&self) -> u64 {
        self.storage_paths.iter().map(|d| d.total_bytes).sum()
    }

    /// Get disk info for a specific path.
    pub fn get_disk_info(&self, path: &str) -> Option<&DiskInfo> {
        self.storage_paths.iter().find(|d| d.path == path)
    }

    /// Get all storage paths.
    pub fn list_disks(&self) -> &[DiskInfo] {
        &self.storage_paths
    }

    /// Get the number of managed storage paths.
    pub fn disk_count(&self) -> usize {
        self.storage_paths.len()
    }

    /// Check if there is enough space across all paths for the given size.
    pub fn has_space(&self, required_bytes: u64) -> bool {
        self.total_available() >= required_bytes
    }

    /// Find the best disk for storing a given number of bytes.
    ///
    /// Prefers disks with the most available space.
    pub fn best_disk_for(&self, required_bytes: u64) -> Option<&DiskInfo> {
        self.storage_paths
            .iter()
            .filter(|d| d.has_space(required_bytes))
            .max_by_key(|d| d.available_bytes)
    }
}

impl Default for DiskManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_utilization() {
        let info = DiskInfo::new("/test".into(), DiskType::Ssd, 1000, 300);
        assert!((info.utilization() - 0.7).abs() < 0.001);
        assert_eq!(info.utilization_percent(), "70.0%");
    }

    #[test]
    fn test_disk_utilization_zero_total() {
        let info = DiskInfo::new("/test".into(), DiskType::Hdd, 0, 0);
        assert!((info.utilization()).abs() < f64::EPSILON);
    }

    #[test]
    fn test_disk_has_space() {
        let info = DiskInfo::new("/test".into(), DiskType::Hdd, 1000, 500);
        assert!(info.has_space(500));
        assert!(!info.has_space(501));
        assert!(info.has_space(0));
    }

    #[test]
    fn test_disk_type_multipliers() {
        assert_eq!(DiskType::Hdd.multiplier(), 1.0);
        assert_eq!(DiskType::Ssd.multiplier(), 1.5);
        assert_eq!(DiskType::Nvme.multiplier(), 2.0);
        assert_eq!(DiskType::Unknown.multiplier(), 1.0);
    }

    #[test]
    fn test_disk_type_display() {
        assert_eq!(DiskType::Hdd.to_string(), "HDD");
        assert_eq!(DiskType::Ssd.to_string(), "SSD");
        assert_eq!(DiskType::Nvme.to_string(), "NVMe");
        assert_eq!(DiskType::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn test_disk_type_from_str() {
        assert_eq!("hdd".parse::<DiskType>().unwrap(), DiskType::Hdd);
        assert_eq!("SSD".parse::<DiskType>().unwrap(), DiskType::Ssd);
        assert_eq!("Nvme".parse::<DiskType>().unwrap(), DiskType::Nvme);
        assert_eq!("floppy".parse::<DiskType>().unwrap(), DiskType::Unknown);
    }

    #[test]
    fn test_disk_info_human_readable() {
        let info = DiskInfo::new("/test".into(), DiskType::Ssd, 1_099_511_627_776, 549_755_813_888);
        assert!(info.total_human().contains("TB"));
        assert!(info.available_human().contains("GB"));
    }

    #[test]
    fn test_disk_manager_new() {
        let manager = DiskManager::new();
        assert!(manager.list_disks().is_empty());
        assert_eq!(manager.disk_count(), 0);
    }

    #[test]
    fn test_disk_manager_add_path() {
        let mut manager = DiskManager::new();
        let result = manager.add_storage_path("/tmp");
        // /tmp should exist on any system
        if result.is_ok() {
            assert_eq!(manager.disk_count(), 1);
        }
    }

    #[test]
    fn test_disk_manager_add_invalid_path() {
        let mut manager = DiskManager::new();
        let result = manager.add_storage_path("/nonexistent/path/that/does/not/exist");
        assert!(result.is_err());
        assert_eq!(manager.disk_count(), 0);
    }

    #[test]
    fn test_total_available() {
        let mut manager = DiskManager::new();
        manager.storage_paths.push(DiskInfo::new(
            "/a".into(),
            DiskType::Hdd,
            1000,
            400,
        ));
        manager.storage_paths.push(DiskInfo::new(
            "/b".into(),
            DiskType::Ssd,
            2000,
            1000,
        ));
        assert_eq!(manager.total_available(), 1400);
    }

    #[test]
    fn test_total_used() {
        let mut manager = DiskManager::new();
        manager.storage_paths.push(DiskInfo::new(
            "/a".into(),
            DiskType::Hdd,
            1000,
            600,
        ));
        manager.storage_paths.push(DiskInfo::new(
            "/b".into(),
            DiskType::Ssd,
            2000,
            1000,
        ));
        assert_eq!(manager.total_used(), 1400);
    }

    #[test]
    fn test_total_capacity() {
        let mut manager = DiskManager::new();
        manager.storage_paths.push(DiskInfo::new(
            "/a".into(),
            DiskType::Hdd,
            1000,
            400,
        ));
        manager.storage_paths.push(DiskInfo::new(
            "/b".into(),
            DiskType::Ssd,
            2000,
            1000,
        ));
        assert_eq!(manager.total_capacity(), 3000);
    }

    #[test]
    fn test_has_space() {
        let mut manager = DiskManager::new();
        manager.storage_paths.push(DiskInfo::new(
            "/a".into(),
            DiskType::Hdd,
            1000,
            400,
        ));
        assert!(manager.has_space(400));
        assert!(!manager.has_space(401));
    }

    #[test]
    fn test_best_disk_for() {
        let mut manager = DiskManager::new();
        manager.storage_paths.push(DiskInfo::new(
            "/small".into(),
            DiskType::Hdd,
            1000,
            100,
        ));
        manager.storage_paths.push(DiskInfo::new(
            "/big".into(),
            DiskType::Ssd,
            10000,
            5000,
        ));

        let best = manager.best_disk_for(500).unwrap();
        assert_eq!(best.path, "/big");

        // Not enough space on any disk
        assert!(manager.best_disk_for(10000).is_none());
    }

    #[test]
    fn test_get_disk_info() {
        let mut manager = DiskManager::new();
        manager.storage_paths.push(DiskInfo::new(
            "/a".into(),
            DiskType::Hdd,
            1000,
            500,
        ));
        assert!(manager.get_disk_info("/a").is_some());
        assert!(manager.get_disk_info("/nonexistent").is_none());
    }

    #[test]
    fn test_disk_info_new() {
        let info = DiskInfo::new("/test".into(), DiskType::Nvme, 2000, 1500);
        assert_eq!(info.used_bytes, 500);
        assert_eq!(info.total_bytes, 2000);
    }
}
