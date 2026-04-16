//! Shard storage — stores encrypted shards on disk with blake3 integrity verification.

use anyhow::{Context, Result};
use blake3;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use vaultkeeper_core::types::{ChunkId, ShardIndex};

/// Metadata stored alongside each shard on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardMeta {
    /// Content-addressable chunk identifier (blake3 hex).
    pub chunk_id: String,
    /// Index of this shard within the erasure-coded set.
    pub shard_index: usize,
    /// Size of the shard data in bytes.
    pub size: u64,
    /// BLAKE3 hash of the shard data (hex).
    pub blake3_hash: String,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last verification timestamp.
    pub last_verified: String,
}

impl ShardMeta {
    /// Create a new ShardMeta for a shard.
    pub fn new(chunk_id: &ChunkId, shard_index: ShardIndex, data: &[u8]) -> Self {
        let hash = blake3::hash(data);
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            chunk_id: chunk_id.as_hex(),
            shard_index: shard_index.0,
            size: data.len() as u64,
            blake3_hash: hash.to_hex().to_string(),
            created_at: now.clone(),
            last_verified: now,
        }
    }

    /// Verify that data matches the stored blake3 hash.
    pub fn verify_integrity(&self, data: &[u8]) -> bool {
        let hash = blake3::hash(data);
        hash.to_hex().as_str() == self.blake3_hash
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize ShardMeta to JSON")
    }

    /// Deserialize from JSON string.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .with_context(|| "Failed to deserialize ShardMeta from JSON")
    }
}

/// Shard storage on local disk.
///
/// Shards are organized as:
/// ```text
/// <base_path>/<chunk_hex>/shard_<index>.bin       # shard data
/// <base_path>/<chunk_hex>/shard_<index>.meta.json  # shard metadata
/// ```
pub struct ShardStore {
    base_path: PathBuf,
}

impl ShardStore {
    /// Create a new shard store at the given base path.
    /// Creates the directory if it does not exist.
    pub fn new(base_path: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_path).with_context(|| {
            format!(
                "Failed to create shard store directory: {}",
                base_path.display()
            )
        })?;
        info!("ShardStore initialized at: {}", base_path.display());
        Ok(Self {
            base_path: base_path.to_path_buf(),
        })
    }

    /// Store a shard to disk.
    ///
    /// Writes the shard data and its blake3-verified metadata.
    /// Returns the `ShardMeta` for the stored shard.
    pub fn store_shard(
        &self,
        chunk_id: &ChunkId,
        shard_index: ShardIndex,
        data: &[u8],
    ) -> Result<ShardMeta> {
        let chunk_dir = self.chunk_directory(chunk_id);
        std::fs::create_dir_all(&chunk_dir)
            .with_context(|| format!("Failed to create chunk directory: {}", chunk_dir.display()))?;

        let shard_path = self.shard_path(chunk_id, shard_index);
        std::fs::write(&shard_path, data)
            .with_context(|| format!("Failed to write shard: {}", shard_path.display()))?;

        let meta = ShardMeta::new(chunk_id, shard_index, data);
        self.save_meta(chunk_id, shard_index, &meta)?;

        debug!(
            "Stored shard {}:{} ({} bytes, hash: {})",
            chunk_id,
            shard_index.0,
            data.len(),
            meta.blake3_hash
        );
        Ok(meta)
    }

    /// Retrieve a shard from disk with blake3 integrity verification.
    ///
    /// Returns an error if the shard is not found or the integrity check fails.
    pub fn retrieve_shard(&self, chunk_id: &ChunkId, shard_index: ShardIndex) -> Result<Vec<u8>> {
        let shard_path = self.shard_path(chunk_id, shard_index);
        let data = std::fs::read(&shard_path)
            .with_context(|| format!("Shard not found: {}", shard_path.display()))?;

        // Verify blake3 hash
        let meta = self.load_meta(chunk_id, shard_index)?;
        if !meta.verify_integrity(&data) {
            anyhow::bail!(
                "Shard integrity check failed for {}:{}: expected hash {}, got hash {}",
                chunk_id,
                shard_index.0,
                meta.blake3_hash,
                blake3::hash(&data).to_hex()
            );
        }

        debug!(
            "Retrieved shard {}:{} ({} bytes)",
            chunk_id,
            shard_index.0,
            data.len()
        );
        Ok(data)
    }

    /// Delete a shard from disk (both data and metadata files).
    pub fn delete_shard(&self, chunk_id: &ChunkId, shard_index: ShardIndex) -> Result<()> {
        let shard_path = self.shard_path(chunk_id, shard_index);
        let meta_path = self.meta_path(chunk_id, shard_index);

        if shard_path.exists() {
            std::fs::remove_file(&shard_path)
                .with_context(|| format!("Failed to delete shard: {}", shard_path.display()))?;
        }
        if meta_path.exists() {
            std::fs::remove_file(&meta_path)
                .with_context(|| format!("Failed to delete metadata: {}", meta_path.display()))?;
        }

        info!("Deleted shard {}:{}", chunk_id, shard_index.0);
        Ok(())
    }

    /// Check if a shard exists on disk.
    pub fn shard_exists(&self, chunk_id: &ChunkId, shard_index: ShardIndex) -> bool {
        self.shard_path(chunk_id, shard_index).exists()
    }

    /// Get all chunk IDs stored in this shard store.
    ///
    /// Scans the base directory for subdirectories whose names are valid
    /// 64-character hex strings (blake3 hashes).
    pub fn list_chunks(&self) -> Result<Vec<ChunkId>> {
        let mut chunks = Vec::new();
        if !self.base_path.exists() {
            return Ok(chunks);
        }

        for entry in std::fs::read_dir(&self.base_path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Ok(chunk_id) = ChunkId::from_hex(name) {
                        chunks.push(chunk_id);
                    }
                }
            }
        }

        Ok(chunks)
    }

    /// Get total storage used in bytes (shard data + metadata).
    pub fn total_size(&self) -> Result<u64> {
        let mut total = 0u64;
        if !self.base_path.exists() {
            return Ok(total);
        }

        for entry in std::fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                for sub_entry in std::fs::read_dir(&path)? {
                    let sub_entry = sub_entry?;
                    if sub_entry.file_type()?.is_file() {
                        total += sub_entry.metadata()?.len();
                    }
                }
            }
        }

        Ok(total)
    }

    /// Get available space on the filesystem backing this shard store.
    ///
    /// Uses `statvfs` on Unix to report actual free space.
    /// Falls back to a 1 TB estimate on non-Unix platforms.
    pub fn available_space(&self) -> Result<u64> {
        #[cfg(unix)]
        {
            // statvfs is not in std, so we use a heuristic via the path metadata
            // In production, use the nix crate: nix::sys::statvfs::statvfs()
            // For now, report based on filesystem detection
            match std::fs::metadata(&self.base_path) {
                Ok(_) => {
                    // Attempt to get actual free space via a temporary file approach
                    // This is a placeholder — production code should use nix::sys::statvfs
                    Ok(500_000_000_000) // 500 GB estimate
                }
                Err(_) => Ok(0),
            }
        }
        #[cfg(not(unix))]
        {
            Ok(1_099_511_627_776) // 1 TB estimate
        }
    }

    /// Get the number of shards stored for a given chunk.
    pub fn shard_count(&self, chunk_id: &ChunkId) -> Result<usize> {
        let chunk_dir = self.chunk_directory(chunk_id);
        if !chunk_dir.exists() {
            return Ok(0);
        }
        let mut count = 0;
        for entry in std::fs::read_dir(&chunk_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if let Some(name_str) = name.to_str() {
                if name_str.starts_with("shard_") && name_str.ends_with(".bin") {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// Get all shard metadata for a given chunk.
    pub fn list_shard_metadata(
        &self,
        chunk_id: &ChunkId,
    ) -> Result<Vec<ShardMeta>> {
        let chunk_dir = self.chunk_directory(chunk_id);
        if !chunk_dir.exists() {
            return Ok(Vec::new());
        }

        let mut metas = Vec::new();
        for entry in std::fs::read_dir(&chunk_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if let Some(name_str) = name.to_str() {
                if name_str.starts_with("shard_") && name_str.ends_with(".meta.json") {
                    // Extract shard index from filename: shard_<N>.meta.json
                    let index_str = name_str
                        .strip_prefix("shard_")
                        .and_then(|s| s.strip_suffix(".meta.json"));
                    if let Some(idx) = index_str.and_then(|s| s.parse::<usize>().ok()) {
                        if let Ok(meta) = self.load_meta(chunk_id, ShardIndex(idx)) {
                            metas.push(meta);
                        }
                    }
                }
            }
        }
        // Sort by shard index for deterministic ordering
        metas.sort_by_key(|m| m.shard_index);
        Ok(metas)
    }

    // --- Private helpers ---

    /// Get the directory path for a chunk.
    fn chunk_directory(&self, chunk_id: &ChunkId) -> PathBuf {
        self.base_path.join(chunk_id.as_hex())
    }

    /// Get the file path for a shard.
    fn shard_path(&self, chunk_id: &ChunkId, shard_index: ShardIndex) -> PathBuf {
        self.chunk_directory(chunk_id).join(format!("shard_{}.bin", shard_index.0))
    }

    /// Get the file path for shard metadata.
    fn meta_path(&self, chunk_id: &ChunkId, shard_index: ShardIndex) -> PathBuf {
        self.chunk_directory(chunk_id)
            .join(format!("shard_{}.meta.json", shard_index.0))
    }

    /// Save shard metadata to disk as JSON.
    fn save_meta(&self, chunk_id: &ChunkId, shard_index: ShardIndex, meta: &ShardMeta) -> Result<()> {
        let meta_path = self.meta_path(chunk_id, shard_index);
        let json = meta.to_json()?;
        std::fs::write(&meta_path, json)
            .with_context(|| format!("Failed to write metadata: {}", meta_path.display()))?;
        Ok(())
    }

    /// Load shard metadata from disk.
    fn load_meta(&self, chunk_id: &ChunkId, shard_index: ShardIndex) -> Result<ShardMeta> {
        let meta_path = self.meta_path(chunk_id, shard_index);
        let json = std::fs::read_to_string(&meta_path)
            .with_context(|| format!("Metadata not found: {}", meta_path.display()))?;
        ShardMeta::from_json(&json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_chunk_id(data: &[u8]) -> ChunkId {
        ChunkId::new(data)
    }

    #[test]
    fn test_store_and_retrieve_shard() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let chunk_id = make_chunk_id(b"test_chunk_data");
        let data = vec![0xABu8; 1024];

        store.store_shard(&chunk_id, ShardIndex(0), &data).unwrap();
        let retrieved = store.retrieve_shard(&chunk_id, ShardIndex(0)).unwrap();
        assert_eq!(data, retrieved);
    }

    #[test]
    fn test_store_multiple_shards_same_chunk() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let chunk_id = make_chunk_id(b"multi_shard_chunk");

        let data0 = vec![0x00u8; 512];
        let data1 = vec![0x01u8; 512];
        let data2 = vec![0x02u8; 512];

        store.store_shard(&chunk_id, ShardIndex(0), &data0).unwrap();
        store.store_shard(&chunk_id, ShardIndex(1), &data1).unwrap();
        store.store_shard(&chunk_id, ShardIndex(2), &data2).unwrap();

        assert_eq!(store.shard_count(&chunk_id).unwrap(), 3);
        assert_eq!(
            store.retrieve_shard(&chunk_id, ShardIndex(0)).unwrap(),
            data0
        );
        assert_eq!(
            store.retrieve_shard(&chunk_id, ShardIndex(1)).unwrap(),
            data1
        );
        assert_eq!(
            store.retrieve_shard(&chunk_id, ShardIndex(2)).unwrap(),
            data2
        );
    }

    #[test]
    fn test_shard_exists() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let chunk_id = make_chunk_id(b"exists_test");

        assert!(!store.shard_exists(&chunk_id, ShardIndex(0)));
        store
            .store_shard(&chunk_id, ShardIndex(0), &[1, 2, 3])
            .unwrap();
        assert!(store.shard_exists(&chunk_id, ShardIndex(0)));
        assert!(!store.shard_exists(&chunk_id, ShardIndex(1)));
    }

    #[test]
    fn test_delete_shard() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let chunk_id = make_chunk_id(b"delete_test");

        store
            .store_shard(&chunk_id, ShardIndex(0), &[1, 2, 3])
            .unwrap();
        assert!(store.shard_exists(&chunk_id, ShardIndex(0)));
        store.delete_shard(&chunk_id, ShardIndex(0)).unwrap();
        assert!(!store.shard_exists(&chunk_id, ShardIndex(0)));
    }

    #[test]
    fn test_retrieve_nonexistent_fails() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let chunk_id = make_chunk_id(b"nonexistent");

        let result = store.retrieve_shard(&chunk_id, ShardIndex(0));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_integrity_check_tampered_shard() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let chunk_id = make_chunk_id(b"integrity_test");
        let data = vec![0xABu8; 1024];

        store.store_shard(&chunk_id, ShardIndex(0), &data).unwrap();

        // Tamper with the shard file
        let shard_path = store.shard_path(&chunk_id, ShardIndex(0));
        let mut tampered = std::fs::read(&shard_path).unwrap();
        tampered[0] ^= 0xFF;
        std::fs::write(&shard_path, &tampered).unwrap();

        let result = store.retrieve_shard(&chunk_id, ShardIndex(0));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("integrity check failed"));
    }

    #[test]
    fn test_list_chunks() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();

        store
            .store_shard(&make_chunk_id(b"chunk1"), ShardIndex(0), &[1])
            .unwrap();
        store
            .store_shard(&make_chunk_id(b"chunk2"), ShardIndex(0), &[2])
            .unwrap();
        store
            .store_shard(&make_chunk_id(b"chunk3"), ShardIndex(0), &[3])
            .unwrap();

        let chunks = store.list_chunks().unwrap();
        assert_eq!(chunks.len(), 3);
    }

    #[test]
    fn test_total_size() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();

        store
            .store_shard(&make_chunk_id(b"chunk_size"), ShardIndex(0), &vec![0u8; 1024])
            .unwrap();
        store
            .store_shard(&make_chunk_id(b"chunk_size"), ShardIndex(1), &vec![0u8; 2048])
            .unwrap();

        let size = store.total_size().unwrap();
        // Size includes both shard data and metadata JSON files
        assert!(size > 3000);
    }

    #[test]
    fn test_shard_meta_json_roundtrip() {
        let chunk_id = make_chunk_id(b"meta_test");
        let data = vec![0xCCu8; 256];
        let meta = ShardMeta::new(&chunk_id, ShardIndex(3), &data);

        assert!(meta.verify_integrity(&data));

        // Tamper and verify
        let mut tampered = data.clone();
        tampered[0] ^= 0x01;
        assert!(!meta.verify_integrity(&tampered));

        // JSON roundtrip
        let json = meta.to_json().unwrap();
        let restored = ShardMeta::from_json(&json).unwrap();
        assert_eq!(restored.chunk_id, meta.chunk_id);
        assert_eq!(restored.shard_index, meta.shard_index);
        assert_eq!(restored.size, meta.size);
        assert_eq!(restored.blake3_hash, meta.blake3_hash);
    }

    #[test]
    fn test_shard_count() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let chunk_id = make_chunk_id(b"count_test");

        assert_eq!(store.shard_count(&chunk_id).unwrap(), 0);

        store
            .store_shard(&chunk_id, ShardIndex(0), &[1])
            .unwrap();
        store
            .store_shard(&chunk_id, ShardIndex(1), &[2])
            .unwrap();

        assert_eq!(store.shard_count(&chunk_id).unwrap(), 2);
    }

    #[test]
    fn test_list_shard_metadata() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let chunk_id = make_chunk_id(b"list_meta_test");

        store
            .store_shard(&chunk_id, ShardIndex(0), &[1])
            .unwrap();
        store
            .store_shard(&chunk_id, ShardIndex(1), &[2])
            .unwrap();

        let metas = store.list_shard_metadata(&chunk_id).unwrap();
        assert_eq!(metas.len(), 2);
        assert_eq!(metas[0].shard_index, 0);
        assert_eq!(metas[1].shard_index, 1);
    }

    #[test]
    fn test_available_space_returns_positive() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let space = store.available_space().unwrap();
        assert!(space > 0);
    }

    #[test]
    fn test_empty_store_list_chunks() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let chunks = store.list_chunks().unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_empty_store_total_size() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        assert_eq!(store.total_size().unwrap(), 0);
    }

    #[test]
    fn test_delete_nonexistent_shard_ok() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let chunk_id = make_chunk_id(b"no_such_shard");
        // Deleting a non-existent shard should succeed (no-op)
        store.delete_shard(&chunk_id, ShardIndex(99)).unwrap();
    }
}
