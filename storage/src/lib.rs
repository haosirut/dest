//! VaultKeeper Storage — Storage management layer.
//!
//! Features: seccomp/cgroups sandboxing (Linux), disk I/O management,
//! shard storage, replication logic, auto-repair on node failure.

pub mod disk;
pub mod replication;
pub mod sandbox;
pub mod shard_store;
