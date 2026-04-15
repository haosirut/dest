//! VaultKeeper Ledger — Local SQLite + async gossip-synced Merkle Ledger.
//!
//! Features: eventual consistency, offline-first, sync on reconnect,
//! conflict resolution by timestamp + majority vote.

pub mod conflict;
pub mod gossip_sync;
pub mod schema;
pub mod store;
