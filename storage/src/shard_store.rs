//! Shard storage — stores encrypted shards on disk.

use anyhow::{Context, Result};
use blake3;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use vaultkeeper_core::{
    ChunkId, ShardIndex, CHUNK_SIZE,
};

/// Metadata stored alongside each shard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardMeta {
    pub chunk_id: String,
    pub shard_index: usize,
    pub size: u64,
    pub blake3_hash: String,
    pub created_at: String,
    pub last_verified: String,
}

/// Shard storage on local disk
pub struct ShardStore {
    base_path: PathBuf,
}

impl ShardStore {
    /// Create a new shard store at the given base path.
    pub fn new(base_path: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_path)
            .with_context(|| format!("Failed to create shard store directory: {}", base_path.display()))?;
        info!("ShardStore initialized at: {}", base_path.display());
        Ok(Self { base_path: base_path.to_path_buf() })
    }

    /// Store a shard to disk.
    pub fn store_shard(&self, chunk_id: &ChunkId, shard_index: ShardIndex, data: &[u8]) -> Result<ShardMeta> {
        let chunk_dir = self.chunk_directory(chunk_id);
        std::fs::create_dir_all(&chunk_dir)?;

        let shard_path = self.shard_path(chunk_id, shard_index);
        std::fs::write(&shard_path, data)
            .with_context(|| format!("Failed to write shard: {}", shard_path.display()))?;

        let hash = blake3::hash(data);
        let meta = ShardMeta {
            chunk_id: chunk_id.to_string(),
            shard_index: shard_index.0,
            size: data.len() as u64,
            blake3_hash: hash.to_hex().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            last_verified: chrono::Utc::now().to_rfc3339(),
        };

        self.save_meta(chunk_id, shard_index, &meta)?;
        debug!("Stored shard {}:{} ({} bytes, hash: {})", chunk_id, shard_index.0, data.len(), hash.to_hex());
        Ok(meta)
    }

    /// Retrieve a shard from disk.
    pub fn retrieve_shard(&self, chunk_id: &ChunkId, shard_index: ShardIndex) -> Result<Vec<u8>> {
        let shard_path = self.shard_path(chunk_id, shard_index);
        let data = std::fs::read(&shard_path)
            .with_context(|| format!("Shard not found: {}", shard_path.display()))?;

        // Verify hash
        let meta = self.load_meta(chunk_id, shard_index)?;
        let hash = blake3::hash(&data);
        if hash.to_hex() != meta.blake3_hash {
            anyhow::bail!("Shard integrity check failed for {}:{}", chunk_id, shard_index.0);
        }

        debug!("Retrieved shard {}:{} ({} bytes)", chunk_id, shard_index.0, data.len());
        Ok(data)
    }

    /// Delete a shard from disk.
    pub fn delete_shard(&self, chunk_id: &ChunkId, shard_index: ShardIndex) -> Result<()> {
        let shard_path = self.shard_path(chunk_id, shard_index);
        let meta_path = self.meta_path(chunk_id, shard_index);

        if shard_path.exists() {
            std::fs::remove_file(&shard_path)?;
        }
        if meta_path.exists() {
            std::fs::remove_file(&meta_path)?;
        }

        info!("Deleted shard {}:{}", chunk_id, shard_index.0);
        Ok(())
    }

    /// Check if a shard exists
    pub fn shard_exists(&self, chunk_id: &ChunkId, shard_index: ShardIndex) -> bool {
        self.shard_path(chunk_id, shard_index).exists()
    }

    /// Get all chunk IDs stored
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

    /// Get total storage used in bytes
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

    /// Get available space (placeholder — in production, query filesystem)
    pub fn available_space(&self) -> Result<u64> {
        // Return a large value as placeholder
        Ok(1_099_511_627_776) // 1 TB
    }

    fn chunk_directory(&self, chunk_id: &ChunkId) -> PathBuf {
        self.base_path.join(chunk_id.as_hex())
    }

    fn shard_path(&self, chunk_id: &ChunkId, shard_index: ShardIndex) -> PathBuf {
        self.chunk_directory(chunk_id).join(format!("shard_{}.bin", shard_index.0))
    }

    fn meta_path(&self, chunk_id: &ChunkId, shard_index: ShardIndex) -> PathBuf {
        self.chunk_directory(chunk_id).join(format!("shard_{}.meta.json", shard_index.0))
    }

    fn save_meta(&self, chunk_id: &ChunkId, shard_index: ShardIndex, meta: &ShardMeta) -> Result<()> {
        let meta_path = self.meta_path(chunk_id, shard_index);
        let json = serde_json::to_string_pretty(meta)?;
        std::fs::write(&meta_path, json)?;
        Ok(())
    }

    fn load_meta(&self, chunk_id: &ChunkId, shard_index: ShardIndex) -> Result<ShardMeta> {
        let meta_path = self.meta_path(chunk_id, shard_index);
        let json = std::fs::read_to_string(&meta_path)?;
        let meta: ShardMeta = serde_json::from_str(&json)?;
        Ok(meta)
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
    fn test_shard_exists() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let chunk_id = make_chunk_id(b"exists_test");

        assert!(!store.shard_exists(&chunk_id, ShardIndex(0)));
        store.store_shard(&chunk_id, ShardIndex(0), &[1, 2, 3]).unwrap();
        assert!(store.shard_exists(&chunk_id, ShardIndex(0)));
    }

    #[test]
    fn test_delete_shard() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();
        let chunk_id = make_chunk_id(b"delete_test");

        store.store_shard(&chunk_id, ShardIndex(0), &[1, 2, 3]).unwrap();
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
    }

    #[test]
    fn test_list_chunks() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();

        store.store_shard(&make_chunk_id(b"chunk1"), ShardIndex(0), &[1]).unwrap();
        store.store_shard(&make_chunk_id(b"chunk2"), ShardIndex(0), &[2]).unwrap();

        let chunks = store.list_chunks().unwrap();
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_total_size() {
        let tmp = TempDir::new().unwrap();
        let store = ShardStore::new(tmp.path()).unwrap();

        store.store_shard(&make_chunk_id(b"chunk_size"), ShardIndex(0), &vec![0u8; 1024]).unwrap();
        store.store_shard(&make_chunk_id(b"chunk_size"), ShardIndex(1), &vec![0u8; 2048]).unwrap();

        let size = store.total_size().unwrap();
        // Size includes both shard data and metadata JSON
        assert!(size > 3000);
    }
}
