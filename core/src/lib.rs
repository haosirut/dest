//! VaultKeeper Core — Cryptographic primitives, chunking, erasure coding, BIP39 recovery, Merkle trees.
//!
//! All encryption is client-side only. Keys NEVER leave the device.

pub mod bip39_recovery;
pub mod chunking;
pub mod encryption;
pub mod erasure;
pub mod merkle;
pub mod padding;
pub mod types;

pub use types::*;
