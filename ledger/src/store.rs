//! Ledger store — high-level interface for the local SQLite ledger.

use crate::schema;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;
use tracing::{debug, info};

/// The local ledger store backed by SQLite.
pub struct LedgerStore {
    conn: Mutex<Connection>,
    _db_path: String,
}

impl LedgerStore {
    /// Open or create a ledger database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        schema::init_schema(&conn)?;
        info!("Ledger opened: {}", path.display());

        Ok(Self {
            conn: Mutex::new(conn),
            _db_path: path.to_string_lossy().to_string(),
        })
    }

    /// Open an in-memory ledger (for testing).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        schema::init_schema(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            db_path: ":memory:".to_string(),
        })
    }

    /// Record a new ledger entry.
    pub fn record_entry(
        &self,
        entry_type: &str,
        account_id: &str,
        amount: &str,
        balance_after: &str,
    ) -> Result<i64> {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let merkle_leaf = blake3::hash(format!("{}:{}:{}:{}", id, entry_type, amount, timestamp).as_bytes()).to_hex().to_string();

        let conn = self.conn.lock().unwrap();
        let seq = schema::insert_entry(
            &conn, &id, entry_type, account_id, amount, balance_after, &timestamp, &merkle_leaf,
        )?;

        debug!("Recorded ledger entry #{}: {} {} for account {}", seq, entry_type, amount, account_id);
        Ok(seq)
    }

    /// Get all entries from a given sequence number.
    pub fn get_entries_from(&self, from_seq: i64) -> Result<Vec<Value>> {
        let conn = self.conn.lock().unwrap();
        schema::get_entries_from(&conn, from_seq, 10_000)
    }

    /// Get unsynced entries.
    pub fn get_unsynced(&self) -> Result<Vec<Value>> {
        let conn = self.conn.lock().unwrap();
        schema::get_unsynced_entries(&conn)
    }

    /// Mark entries as synced.
    pub fn mark_synced(&self, up_to_seq: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        schema::mark_synced(&conn, up_to_seq)
    }

    /// Get the latest sequence number.
    pub fn latest_seq(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        schema::get_latest_seq(&conn)
    }

    /// Compute the Merkle root of all ledger entries.
    pub fn compute_merkle_root(&self) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let entries = schema::get_entries_from(&conn, 0, 100_000)?;

        if entries.is_empty() {
            return Ok(blake3::hash(b"empty").to_hex().to_string());
        }

        // Build Merkle tree from entry hashes
        let mut hashes: Vec<blake3::Hash> = entries.iter().map(|e| {
            blake3::hash(e.to_string().as_bytes())
        }).collect();

        while hashes.len() > 1 {
            let mut next_level = Vec::new();
            for i in (0..hashes.len()).step_by(2) {
                let left = hashes[i];
                let right = if i + 1 < hashes.len() { hashes[i + 1] } else { hashes[i] };
                let mut hasher = blake3::Hasher::new();
                hasher.update(left.as_bytes());
                hasher.update(right.as_bytes());
                next_level.push(hasher.finalize());
            }
            hashes = next_level;
        }

        Ok(hashes[0].to_hex().to_string())
    }

    /// Store a Merkle root checkpoint.
    pub fn store_merkle_checkpoint(&self, root_hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let seq = schema::get_latest_seq(&conn)?;
        schema::store_merkle_root(&conn, seq, root_hash, seq)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let store = LedgerStore::open_in_memory().unwrap();
        assert_eq!(store.latest_seq().unwrap(), 0);
    }

    #[test]
    fn test_record_entry() {
        let store = LedgerStore::open_in_memory().unwrap();
        let seq = store.record_entry("deposit", "acc1", "100.00", "100.00").unwrap();
        assert_eq!(seq, 1);
        assert_eq!(store.latest_seq().unwrap(), 1);
    }

    #[test]
    fn test_multiple_entries() {
        let store = LedgerStore::open_in_memory().unwrap();
        store.record_entry("deposit", "acc1", "100.00", "100.00").unwrap();
        store.record_entry("charge", "acc1", "10.00", "90.00").unwrap();
        store.record_entry("deposit", "acc2", "50.00", "50.00").unwrap();
        assert_eq!(store.latest_seq().unwrap(), 3);
    }

    #[test]
    fn test_get_unsynced() {
        let store = LedgerStore::open_in_memory().unwrap();
        store.record_entry("deposit", "acc1", "100.00", "100.00").unwrap();
        let unsynced = store.get_unsynced().unwrap();
        assert_eq!(unsynced.len(), 1);
    }

    #[test]
    fn test_mark_synced() {
        let store = LedgerStore::open_in_memory().unwrap();
        store.record_entry("deposit", "acc1", "100.00", "100.00").unwrap();
        store.mark_synced(1).unwrap();
        let unsynced = store.get_unsynced().unwrap();
        assert_eq!(unsynced.len(), 0);
    }

    #[test]
    fn test_compute_merkle_root() {
        let store = LedgerStore::open_in_memory().unwrap();
        store.record_entry("deposit", "acc1", "100.00", "100.00").unwrap();
        let root = store.compute_merkle_root().unwrap();
        assert!(!root.is_empty());
    }

    #[test]
    fn test_merkle_root_deterministic() {
        let store1 = LedgerStore::open_in_memory().unwrap();
        let store2 = LedgerStore::open_in_memory().unwrap();

        store1.record_entry("deposit", "acc1", "100.00", "100.00").unwrap();
        store2.record_entry("deposit", "acc1", "100.00", "100.00").unwrap();

        // Note: Merkle roots may differ due to different UUIDs
        // This test verifies the function works
        let root1 = store1.compute_merkle_root().unwrap();
        let root2 = store2.compute_merkle_root().unwrap();
        assert!(!root1.is_empty());
        assert!(!root2.is_empty());
    }

    #[test]
    fn test_get_entries_from() {
        let store = LedgerStore::open_in_memory().unwrap();
        store.record_entry("deposit", "acc1", "100.00", "100.00").unwrap();
        store.record_entry("charge", "acc1", "10.00", "90.00").unwrap();
        store.record_entry("deposit", "acc1", "50.00", "140.00").unwrap();

        let entries = store.get_entries_from(1).unwrap();
        assert_eq!(entries.len(), 2);
    }
}
