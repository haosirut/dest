use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// A single ledger entry record stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub seq: i64,
    pub account_id: String,
    pub entry_type: String,
    pub amount: String, // stored as decimal string
    pub data_json: String,
    pub created_at: String,
    pub synced: bool,
}

/// A stored Merkle root checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleRootRecord {
    pub id: i64,
    pub root_hash: String,
    pub seq_from: i64,
    pub seq_to: i64,
    pub created_at: String,
}

/// Initializes the SQLite schema with all required tables.
///
/// Tables created:
/// - `ledger_entries`: individual billing/transaction entries
/// - `merkle_roots`: periodic Merkle root checkpoints
/// - `ledger_meta`: key-value metadata store
pub fn init_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS ledger_entries (
            seq          INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id   TEXT NOT NULL,
            entry_type   TEXT NOT NULL,
            amount       TEXT NOT NULL,
            data_json    TEXT NOT NULL DEFAULT '{}',
            created_at   TEXT NOT NULL DEFAULT (datetime('now')),
            synced       INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS merkle_roots (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            root_hash   TEXT NOT NULL,
            seq_from    INTEGER NOT NULL,
            seq_to      INTEGER NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS ledger_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ledger_entries_account ON ledger_entries(account_id);
        CREATE INDEX IF NOT EXISTS idx_ledger_entries_synced ON ledger_entries(synced);
        CREATE INDEX IF NOT EXISTS idx_merkle_roots_seq ON merkle_roots(seq_from, seq_to);
        ",
    )?;
    Ok(())
}

/// Inserts a new ledger entry and returns its sequence number.
pub fn insert_entry(
    conn: &Connection,
    account_id: &str,
    entry_type: &str,
    amount: &str,
    data_json: &str,
) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO ledger_entries (account_id, entry_type, amount, data_json) VALUES (?1, ?2, ?3, ?4)",
        params![account_id, entry_type, amount, data_json],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Returns the latest sequence number in the ledger, or 0 if empty.
pub fn get_latest_seq(conn: &Connection) -> Result<i64, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT MAX(seq) FROM ledger_entries")?;
    let seq: Option<i64> = stmt.query_row([], |row| row.get(0))?;
    Ok(seq.unwrap_or(0))
}

/// Returns all entries with seq > `from_seq`, ordered by seq ascending.
pub fn get_entries_from(conn: &Connection, from_seq: i64) -> Result<Vec<LedgerEntry>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT seq, account_id, entry_type, amount, data_json, created_at, synced FROM ledger_entries WHERE seq > ?1 ORDER BY seq ASC"
    )?;
    let entries = stmt
        .query_map(params![from_seq], |row| {
            Ok(LedgerEntry {
                seq: row.get(0)?,
                account_id: row.get(1)?,
                entry_type: row.get(2)?,
                amount: row.get(3)?,
                data_json: row.get(4)?,
                created_at: row.get(5)?,
                synced: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

/// Marks entries with seq <= `up_to_seq` as synced.
pub fn mark_synced(conn: &Connection, up_to_seq: i64) -> Result<usize, rusqlite::Error> {
    let count = conn.execute(
        "UPDATE ledger_entries SET synced = 1 WHERE seq <= ?1 AND synced = 0",
        params![up_to_seq],
    )?;
    Ok(count)
}

/// Returns all unsynced entries, ordered by seq ascending.
pub fn get_unsynced_entries(conn: &Connection) -> Result<Vec<LedgerEntry>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT seq, account_id, entry_type, amount, data_json, created_at, synced FROM ledger_entries WHERE synced = 0 ORDER BY seq ASC"
    )?;
    let entries = stmt
        .query_map([], |row| {
            Ok(LedgerEntry {
                seq: row.get(0)?,
                account_id: row.get(1)?,
                entry_type: row.get(2)?,
                amount: row.get(3)?,
                data_json: row.get(4)?,
                created_at: row.get(5)?,
                synced: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

/// Stores a Merkle root checkpoint.
pub fn store_merkle_root(
    conn: &Connection,
    root_hash: &str,
    seq_from: i64,
    seq_to: i64,
) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO merkle_roots (root_hash, seq_from, seq_to) VALUES (?1, ?2, ?3)",
        params![root_hash, seq_from, seq_to],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Returns the latest Merkle root record, or None if none exist.
pub fn get_latest_merkle_root(conn: &Connection) -> Result<Option<MerkleRootRecord>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, root_hash, seq_from, seq_to, created_at FROM merkle_roots ORDER BY id DESC LIMIT 1"
    )?;
    let result = stmt.query_row([], |row| {
        Ok(MerkleRootRecord {
            id: row.get(0)?,
            root_hash: row.get(1)?,
            seq_from: row.get(2)?,
            seq_to: row.get(3)?,
            created_at: row.get(4)?,
        })
    });
    match result {
        Ok(record) => Ok(Some(record)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn test_init_schema_creates_tables() {
        let conn = setup_db();
        // Verify tables exist by querying them
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").unwrap();
        let tables: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert!(tables.contains(&"ledger_entries".to_string()));
        assert!(tables.contains(&"merkle_roots".to_string()));
        assert!(tables.contains(&"ledger_meta".to_string()));
    }

    #[test]
    fn test_insert_and_get_entry() {
        let conn = setup_db();
        let seq = insert_entry(&conn, "acc-1", "deposit", "100.0", r#"{"desc":"top-up"}"#).unwrap();
        assert_eq!(seq, 1);
        let entries = get_entries_from(&conn, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].account_id, "acc-1");
        assert_eq!(entries[0].amount, "100.0");
    }

    #[test]
    fn test_get_latest_seq_empty() {
        let conn = setup_db();
        assert_eq!(get_latest_seq(&conn).unwrap(), 0);
    }

    #[test]
    fn test_get_latest_seq_multiple() {
        let conn = setup_db();
        insert_entry(&conn, "acc-1", "deposit", "10", "{}").unwrap();
        insert_entry(&conn, "acc-2", "charge", "5", "{}").unwrap();
        assert_eq!(get_latest_seq(&conn).unwrap(), 2);
    }

    #[test]
    fn test_get_entries_from_offset() {
        let conn = setup_db();
        insert_entry(&conn, "acc-1", "deposit", "10", "{}").unwrap();
        insert_entry(&conn, "acc-2", "deposit", "20", "{}").unwrap();
        insert_entry(&conn, "acc-3", "deposit", "30", "{}").unwrap();
        let entries = get_entries_from(&conn, 1).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].seq, 2);
        assert_eq!(entries[1].seq, 3);
    }

    #[test]
    fn test_mark_synced() {
        let conn = setup_db();
        insert_entry(&conn, "acc-1", "deposit", "10", "{}").unwrap();
        insert_entry(&conn, "acc-2", "deposit", "20", "{}").unwrap();
        insert_entry(&conn, "acc-3", "deposit", "30", "{}").unwrap();

        let unsynced = get_unsynced_entries(&conn).unwrap();
        assert_eq!(unsynced.len(), 3);

        mark_synced(&conn, 2).unwrap();
        let unsynced = get_unsynced_entries(&conn).unwrap();
        assert_eq!(unsynced.len(), 1);
        assert_eq!(unsynced[0].seq, 3);
    }

    #[test]
    fn test_store_and_get_merkle_root() {
        let conn = setup_db();
        let id = store_merkle_root(&conn, "abc123", 1, 10).unwrap();
        assert_eq!(id, 1);

        let root = get_latest_merkle_root(&conn).unwrap();
        assert!(root.is_some());
        let root = root.unwrap();
        assert_eq!(root.root_hash, "abc123");
        assert_eq!(root.seq_from, 1);
        assert_eq!(root.seq_to, 10);
    }

    #[test]
    fn test_get_latest_merkle_root_empty() {
        let conn = setup_db();
        let root = get_latest_merkle_root(&conn).unwrap();
        assert!(root.is_none());
    }
}
