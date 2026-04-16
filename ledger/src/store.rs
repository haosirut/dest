use rusqlite::Connection;
use std::path::Path;

use crate::schema;

/// A persistent ledger store backed by SQLite.
pub struct LedgerStore {
    conn: Connection,
}

impl LedgerStore {
    /// Opens (or creates) a ledger database at the given file path.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        schema::init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Opens an in-memory ledger database (useful for testing).
    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        schema::init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Records a new ledger entry, returning its sequence number.
    pub fn record_entry(
        &self,
        account_id: &str,
        entry_type: &str,
        amount: &str,
        data_json: &str,
    ) -> Result<i64, rusqlite::Error> {
        schema::insert_entry(&self.conn, account_id, entry_type, amount, data_json)
    }

    /// Returns all entries with seq > `from_seq`, ordered by seq ascending.
    pub fn get_entries_from(&self, from_seq: i64) -> Result<Vec<schema::LedgerEntry>, rusqlite::Error> {
        schema::get_entries_from(&self.conn, from_seq)
    }

    /// Returns all unsynced entries.
    pub fn get_unsynced(&self) -> Result<Vec<schema::LedgerEntry>, rusqlite::Error> {
        schema::get_unsynced_entries(&self.conn)
    }

    /// Marks entries with seq <= `up_to_seq` as synced.
    pub fn mark_synced(&self, up_to_seq: i64) -> Result<usize, rusqlite::Error> {
        schema::mark_synced(&self.conn, up_to_seq)
    }

    /// Returns the latest sequence number, or 0 if the ledger is empty.
    pub fn latest_seq(&self) -> Result<i64, rusqlite::Error> {
        schema::get_latest_seq(&self.conn)
    }

    /// Computes a Merkle root (BLAKE3 hash) over all entries in the ledger.
    ///
    /// The hash is computed by hashing each entry's serialized representation
    /// in sequence order, then hashing the concatenation of all individual hashes.
    pub fn compute_merkle_root(&self) -> Result<String, rusqlite::Error> {
        let entries = schema::get_entries_from(&self.conn, 0)?;
        if entries.is_empty() {
            // Return the hash of empty input
            return Ok(hex::encode(blake3::hash(b"").as_bytes()));
        }

        let mut hasher = blake3::Hasher::new();
        for entry in &entries {
            // Create a canonical representation of the entry for hashing
            let canonical = format!(
                "{}:{}:{}:{}:{}:{}",
                entry.seq,
                entry.account_id,
                entry.entry_type,
                entry.amount,
                entry.data_json,
                entry.created_at
            );
            hasher.update(canonical.as_bytes());
        }
        Ok(hex::encode(hasher.finalize().as_bytes()))
    }

    /// Stores a Merkle root checkpoint for the current state of the ledger.
    pub fn store_merkle_checkpoint(&self) -> Result<(String, i64), rusqlite::Error> {
        let root_hash = self.compute_merkle_root()?;
        let latest = self.latest_seq()?;
        let seq_from = if latest > 0 {
            // Get the previous checkpoint's seq_to to determine range
            match schema::get_latest_merkle_root(&self.conn)? {
                Some(prev) => prev.seq_to + 1,
                None => 1,
            }
        } else {
            0
        };
        let id = schema::store_merkle_root(&self.conn, &root_hash, seq_from, latest)?;
        Ok((root_hash, id))
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
        let seq = store.record_entry("acc-1", "deposit", "100.0", r#"{}"#).unwrap();
        assert_eq!(seq, 1);
        assert_eq!(store.latest_seq().unwrap(), 1);
    }

    #[test]
    fn test_record_multiple_entries() {
        let store = LedgerStore::open_in_memory().unwrap();
        store.record_entry("acc-1", "deposit", "50", "{}").unwrap();
        store.record_entry("acc-1", "charge", "10", "{}").unwrap();
        store.record_entry("acc-2", "deposit", "200", "{}").unwrap();
        assert_eq!(store.latest_seq().unwrap(), 3);
    }

    #[test]
    fn test_get_entries_from() {
        let store = LedgerStore::open_in_memory().unwrap();
        store.record_entry("acc-1", "a", "1", "{}").unwrap();
        store.record_entry("acc-2", "b", "2", "{}").unwrap();
        store.record_entry("acc-3", "c", "3", "{}").unwrap();
        let entries = store.get_entries_from(1).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].seq, 2);
    }

    #[test]
    fn test_get_unsynced() {
        let store = LedgerStore::open_in_memory().unwrap();
        store.record_entry("a", "b", "1", "{}").unwrap();
        store.record_entry("c", "d", "2", "{}").unwrap();
        let unsynced = store.get_unsynced().unwrap();
        assert_eq!(unsynced.len(), 2);
        store.mark_synced(1).unwrap();
        let unsynced = store.get_unsynced().unwrap();
        assert_eq!(unsynced.len(), 1);
        assert_eq!(unsynced[0].seq, 2);
    }

    #[test]
    fn test_mark_synced() {
        let store = LedgerStore::open_in_memory().unwrap();
        store.record_entry("a", "b", "1", "{}").unwrap();
        store.record_entry("c", "d", "2", "{}").unwrap();
        let count = store.mark_synced(2).unwrap();
        assert_eq!(count, 2);
        assert_eq!(store.get_unsynced().unwrap().len(), 0);
    }

    #[test]
    fn test_compute_merkle_root_empty() {
        let store = LedgerStore::open_in_memory().unwrap();
        let root = store.compute_merkle_root().unwrap();
        // Hash of empty bytes
        assert_eq!(root, hex::encode(blake3::hash(b"").as_bytes()));
    }

    #[test]
    fn test_compute_merkle_root_deterministic() {
        let store1 = LedgerStore::open_in_memory().unwrap();
        let store2 = LedgerStore::open_in_memory().unwrap();

        store1.record_entry("acc-1", "deposit", "100", r#"{"x":1}"#).unwrap();
        store2.record_entry("acc-1", "deposit", "100", r#"{"x":1}"#).unwrap();

        let root1 = store1.compute_merkle_root().unwrap();
        let root2 = store2.compute_merkle_root().unwrap();
        assert_eq!(root1, root2);
    }

    #[test]
    fn test_compute_merkle_root_changes_with_data() {
        let store = LedgerStore::open_in_memory().unwrap();
        let root1 = store.compute_merkle_root().unwrap();
        store.record_entry("acc-1", "deposit", "100", "{}").unwrap();
        let root2 = store.compute_merkle_root().unwrap();
        assert_ne!(root1, root2);
    }

    #[test]
    fn test_store_merkle_checkpoint() {
        let store = LedgerStore::open_in_memory().unwrap();
        store.record_entry("acc-1", "deposit", "100", "{}").unwrap();
        let (root_hash, id) = store.store_merkle_checkpoint().unwrap();
        assert_eq!(id, 1);
        assert!(!root_hash.is_empty());

        // Verify it was stored
        let record = schema::get_latest_merkle_root(&store.conn).unwrap();
        assert!(record.is_some());
        assert_eq!(record.unwrap().root_hash, root_hash);
    }
}
