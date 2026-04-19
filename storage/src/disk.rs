//! Disk management — track available space, disk type detection, I/O operations.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info};

/// Disk type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskType {
    Hdd,
    Ssd,
    Nvme,
    Unknown,
}

impl DiskType {
    /// Get the billing multiplier for this disk type
    pub fn multiplier(&self) -> f64 {
        match self {
            DiskType::Hdd => 1.0,
            DiskType::Ssd => 1.5,
            DiskType::Nvme => 2.0,
            DiskType::Unknown => 1.0,
        }
    }
}

/// Disk information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub path: String,
    pub disk_type: DiskType,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
}

impl DiskInfo {
    /// Get disk utilization percentage (0.0 to 1.0)
    pub fn utilization(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        self.used_bytes as f64 / self.total_bytes as f64
    }

    /// Check if disk has enough space
    pub fn has_space(&self, required_bytes: u64) -> bool {
        self.available_bytes >= required_bytes
    }
}

/// Disk manager — monitors storage resources
pub struct DiskManager {
    storage_paths: Vec<DiskInfo>,
}

impl DiskManager {
    /// Create a new disk manager with the given storage paths.
    pub fn new() -> Self {
        Self {
            storage_paths: Vec::new(),
        }
    }

    /// Add a storage path to monitor
    pub fn add_storage_path(&mut self, path: &str) -> Result<()> {
        let info = self.probe_disk(path)?;
        info!("Added storage path: {} ({:?}, {} GB available)",
            path, info.disk_type, info.available_bytes / 1_073_741_824);
        self.storage_paths.push(info);
        Ok(())
    }

    /// Probe a disk path for information
    pub fn probe_disk(&self, path: &str) -> Result<DiskInfo> {
        let p = Path::new(path);
        if !p.exists() {
            anyhow::bail!("Path does not exist: {}", path);
        }

        // Use std::fs to get filesystem info
        // In production, this would use nix::sys::statvfs on Linux
        let total = 1_099_511_627_776u64; // Placeholder: 1 TB
        let used = 0u64;
        let available = total - used;

        let disk_type = self.detect_disk_type(path);

        Ok(DiskInfo {
            path: path.to_string(),
            disk_type,
            total_bytes: total,
            available_bytes: available,
            used_bytes: used,
        })
    }

    /// Detect disk type (simplified — in production, parse /sys/block/...)
    fn detect_disk_type(&self, _path: &str) -> DiskType {
        // Placeholder: default to HDD
        // In production: check /sys/block/<dev>/queue/rotational
        //   rotational=1 -> HDD, rotational=0 -> SSD/NVMe
        // Then check /sys/block/<dev>/queue/nr_requests for NVMe detection
        DiskType::Hdd
    }

    /// Get total available space across all storage paths
    pub fn total_available(&self) -> u64 {
        self.storage_paths.iter().map(|d| d.available_bytes).sum()
    }

    /// Get total used space across all storage paths
    pub fn total_used(&self) -> u64 {
        self.storage_paths.iter().map(|d| d.used_bytes).sum()
    }

    /// Get disk info for a specific path
    pub fn get_disk_info(&self, path: &str) -> Option<&DiskInfo> {
        self.storage_paths.iter().find(|d| d.path == path)
    }

    /// Get all storage paths
    pub fn list_disks(&self) -> &[DiskInfo] {
        &self.storage_paths
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
        let info = DiskInfo {
            path: "/test".to_string(),
            disk_type: DiskType::Ssd,
            total_bytes: 1000,
            available_bytes: 300,
            used_bytes: 700,
        };
        assert!((info.utilization() - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_disk_has_space() {
        let info = DiskInfo {
            path: "/test".to_string(),
            disk_type: DiskType::Hdd,
            total_bytes: 1000,
            available_bytes: 500,
            used_bytes: 500,
        };
        assert!(info.has_space(500));
        assert!(!info.has_space(501));
    }

    #[test]
    fn test_disk_manager_new() {
        let manager = DiskManager::new();
        assert!(manager.list_disks().is_empty());
    }

    #[test]
    fn test_disk_manager_add_path() {
        let mut manager = DiskManager::new();
        let result = manager.add_storage_path("/tmp");
        // /tmp should exist on any system
        if result.is_ok() {
            assert!(!manager.list_disks().is_empty());
        }
    }

    #[test]
    fn test_disk_type_multipliers() {
        assert_eq!(DiskType::Hdd.multiplier(), 1.0);
        assert_eq!(DiskType::Ssd.multiplier(), 1.5);
        assert_eq!(DiskType::Nvme.multiplier(), 2.0);
    }

    #[test]
    fn test_total_available() {
        let mut manager = DiskManager::new();
        manager.storage_paths.push(DiskInfo {
            path: "/a".into(),
            disk_type: DiskType::Hdd,
            total_bytes: 1000,
            available_bytes: 400,
            used_bytes: 600,
        });
        manager.storage_paths.push(DiskInfo {
            path: "/b".into(),
            disk_type: DiskType::Ssd,
            total_bytes: 2000,
            available_bytes: 1000,
            used_bytes: 1000,
        });
        assert_eq!(manager.total_available(), 1400);
    }
}
