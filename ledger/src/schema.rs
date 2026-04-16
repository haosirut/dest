//! SQLite database schema for the local ledger.

use anyhow::Result;
use rusqlite::{Connection, params};
use tracing::info;

/// Initialize the database schema.
pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;"
    )?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ledger_entries (
            seq INTEGER PRIMARY KEY AUTOINCREMENT,
            id TEXT NOT NULL UNIQUE,
            entry_type TEXT NOT NULL,
            account_id TEXT NOT NULL,
            amount TEXT NOT NULL,
            balance_after TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            merkle_leaf TEXT NOT NULL,
            synced INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_ledger_account ON ledger_entries(account_id);
        CREATE INDEX IF NOT EXISTS idx_ledger_seq ON ledger_entries(seq);
        CREATE INDEX IF NOT EXISTS idx_ledger_synced ON ledger_entries(synced);

        CREATE TABLE IF NOT EXISTS merkle_roots (
            seq INTEGER PRIMARY KEY,
            root_hash TEXT NOT NULL,
            entry_count INTEGER NOT NULL,
            timestamp TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ledger_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        INSERT OR IGNORE INTO ledger_meta (key, value) VALUES ('schema_version', '1');
        INSERT OR IGNORE INTO ledger_meta (key, value) VALUES ('last_sync_seq', '0');
        INSERT OR IGNORE INTO ledger_meta (key, value) VALUES ('local_peer_id', '');
        "
    )?;

    info!("Ledger schema initialized");
    Ok(())
}

/// Insert a new ledger entry and return its sequence number.
pub fn insert_entry(
    conn: &Connection,
    id: &str,
    entry_type: &str,
    account_id: &str,
    amount: &str,
    balance_after: &str,
    timestamp: &str,
    merkle_leaf: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO ledger_entries (id, entry_type, account_id, amount, balance_after, timestamp, merkle_leaf)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, entry_type, account_id, amount, balance_after, timestamp, merkle_leaf],
    )?;

    let seq = conn.last_insert_rowid();
    Ok(seq)
}

/// Get the latest sequence number.
pub fn get_latest_seq(conn: &Connection) -> Result<i64> {
    let seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM ledger_entries",
        [],
        |row| row.get(0),
    )?;
    Ok(seq)
}

/// Get entries from a given sequence number.
pub fn get_entries_from(conn: &Connection, from_seq: i64, limit: usize) -> Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT seq, id, entry_type, account_id, amount, balance_after, timestamp, merkle_leaf
         FROM ledger_entries WHERE seq > ?1 ORDER BY seq ASC LIMIT ?2"
    )?;

    let entries: Vec<serde_json::Value> = stmt.query_map(params![from_seq, limit as i64], |row| {
        let seq: i64 = row.get(0)?;
        let id: String = row.get(1)?;
        let entry_type: String = row.get(2)?;
        let account_id: String = row.get(3)?;
        let amount: String = row.get(4)?;
        let balance_after: String = row.get(5)?;
        let timestamp: String = row.get(6)?;
        let merkle_leaf: String = row.get(7)?;

        Ok(serde_json::json!({
            "seq": seq,
            "id": id,
            "entry_type": entry_type,
            "account_id": account_id,
            "amount": amount,
            "balance_after": balance_after,
            "timestamp": timestamp,
            "merkle_leaf": merkle_leaf,
        }))
    })?.collect::<Result<Vec<_>, _>>()?;

    Ok(entries)
}

/// Mark entries as synced up to a given sequence number.
pub fn mark_synced(conn: &Connection, up_to_seq: i64) -> Result<()> {
    conn.execute(
        "UPDATE ledger_entries SET synced = 1 WHERE seq <= ?1 AND synced = 0",
        params![up_to_seq],
    )?;
    Ok(())
}

/// Get unsynced entries.
pub fn get_unsynced_entries(conn: &Connection) -> Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT seq, id, entry_type, account_id, amount, balance_after, timestamp, merkle_leaf
         FROM ledger_entries WHERE synced = 0 ORDER BY seq ASC"
    )?;

    let entries: Vec<serde_json::Value> = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "seq": row.get::<_, i64>(0)?,
            "id": row.get::<_, String>(1)?,
            "entry_type": row.get::<_, String>(2)?,
            "account_id": row.get::<_, String>(3)?,
            "amount": row.get::<_, String>(4)?,
            "balance_after": row.get::<_, String>(5)?,
            "timestamp": row.get::<_, String>(6)?,
            "merkle_leaf": row.get::<_, String>(7)?,
        }))
    })?.collect::<Result<Vec<_>, _>>()?;

    Ok(entries)
}

/// Store a Merkle root checkpoint.
pub fn store_merkle_root(conn: &Connection, seq: i64, root_hash: &str, entry_count: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO merkle_roots (seq, root_hash, entry_count, timestamp) VALUES (?1, ?2, ?3, datetime('now'))",
        params![seq, root_hash, entry_count],
    )?;
    Ok(())
}

/// Get the latest Merkle root.
pub fn get_latest_merkle_root(conn: &Connection) -> Result<Option<(i64, String)>> {
    let result = conn.query_row(
        "SELECT seq, root_hash FROM merkle_roots ORDER BY seq DESC LIMIT 1",
        [],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
    );

    match result {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
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
    fn test_init_schema() {
        let conn = setup_db();
        // Verify tables exist
        let result: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('ledger_entries', 'merkle_roots', 'ledger_meta')",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(result, 3);
    }

    #[test]
    fn test_insert_and_get_entry() {
        let conn = setup_db();
        let seq = insert_entry(
            &conn, "tx1", "deposit", "acc1", "100.00", "100.00", "2024-01-01T00:00:00Z", "leaf_hash_1"
        ).unwrap();
        assert_eq!(seq, 1);
        assert_eq!(get_latest_seq(&conn).unwrap(), 1);
    }

    #[test]
    fn test_get_entries_from() {
        let conn = setup_db();
        insert_entry(&conn, "tx1", "deposit", "acc1", "100.00", "100.00", "2024-01-01T00:00:00Z", "leaf1").unwrap();
        insert_entry(&conn, "tx2", "charge", "acc1", "10.00", "90.00", "2024-01-01T01:00:00Z", "leaf2").unwrap();
        insert_entry(&conn, "tx3", "deposit", "acc1", "50.00", "140.00", "2024-01-01T02:00:00Z", "leaf3").unwrap();

        let entries = get_entries_from(&conn, 1, 10).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_mark_synced() {
        let conn = setup_db();
        insert_entry(&conn, "tx1", "deposit", "acc1", "100.00", "100.00", "2024-01-01T00:00:00Z", "leaf1").unwrap();
        insert_entry(&conn, "tx2", "charge", "acc1", "10.00", "90.00", "2024-01-01T01:00:00Z", "leaf2").unwrap();

        let unsynced = get_unsynced_entries(&conn).unwrap();
        assert_eq!(unsynced.len(), 2);

        mark_synced(&conn, 1).unwrap();
        let unsynced = get_unsynced_entries(&conn).unwrap();
        assert_eq!(unsynced.len(), 1);
    }

    #[test]
    fn test_merkle_root_storage() {
        let conn = setup_db();
        store_merkle_root(&conn, 5, "root_hash_abc", 5).unwrap();

        let latest = get_latest_merkle_root(&conn).unwrap();
        assert_eq!(latest, Some((5, "root_hash_abc".to_string())));
    }

    #[test]
    fn test_get_latest_seq_empty() {
        let conn = setup_db();
        assert_eq!(get_latest_seq(&conn).unwrap(), 0);
    }

    #[test]
    fn test_schema_version() {
        let conn = setup_db();
        let version: String = conn.query_row(
            "SELECT value FROM ledger_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(version, "1");
    }
}
