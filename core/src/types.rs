use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;

/// 4 MiB chunk size as specified
pub const CHUNK_SIZE: usize = 4 * 1024 * 1024;

/// Fixed block size for encrypted output (4 MiB + random padding)
pub const BLOCK_SIZE: usize = CHUNK_SIZE;

/// Number of data shards for Reed-Solomon
pub const RS_DATA_SHARDS: usize = 3;

/// Number of parity shards for Reed-Solomon (total 5)
pub const RS_PARITY_SHARDS: usize = 2;

/// Total shards
pub const RS_TOTAL_SHARDS: usize = RS_DATA_SHARDS + RS_PARITY_SHARDS;

/// Master key length (256-bit)
pub const KEY_LEN: usize = 32;

/// Nonce length for XChaCha20-Poly1305
pub const NONCE_LEN: usize = 24;

/// Auth tag length
pub const TAG_LEN: usize = 16;

/// Argon2id salt length
pub const SALT_LEN: usize = 32;

/// Argon2id output key length
pub const DERIVED_KEY_LEN: usize = 32;

/// Heartbeat interval: 15 minutes
pub const HEARTBEAT_INTERVAL_SECS: u64 = 900;

/// Replication timeout: 10 minutes for auto-replication on node failure
pub const REPLICATION_TIMEOUT_SECS: u64 = 600;

/// Export window after freeze: 48 hours
pub const EXPORT_WINDOW_SECS: u64 = 48 * 3600;

/// BIP39 word count options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MnemonicWordCount {
    Twelve = 12,
    TwentyFour = 24,
}

/// Identifier for a chunk of data
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkId(pub [u8; 32]);

impl ChunkId {
    pub fn new(data: &[u8]) -> Self {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(data);
        let hash = hasher.finalize();
        let mut id = [0u8; 32];
        id.copy_from_slice(hash.as_bytes());
        Self(id)
    }

    pub fn as_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(hex_str: &str) -> anyhow::Result<Self> {
        let bytes = hex::decode(hex_str)?;
        if bytes.len() != 32 {
            anyhow::bail!("ChunkId must be 32 bytes");
        }
        let mut id = [0u8; 32];
        id.copy_from_slice(&bytes);
        Ok(Self(id))
    }
}

impl fmt::Display for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_hex())
    }
}

/// Shard index within erasure-coded set
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardIndex(pub usize);

/// A single encrypted shard ready for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedShard {
    pub chunk_id: ChunkId,
    pub shard_index: ShardIndex,
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
    pub tag: [u8; TAG_LEN],
}

/// Challenge for Proof-of-Storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageChallenge {
    pub chunk_id: ChunkId,
    pub leaf_indices: Vec<u64>,
    pub nonce: [u8; NONCE_LEN],
    pub timestamp: u64,
}

/// Response to Proof-of-Storage challenge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageProof {
    pub chunk_id: ChunkId,
    pub leaf_indices: Vec<u64>,
    pub leaves: Vec<Vec<u8>>,
    pub proofs: Vec<Vec<blake3::Hash>>,
    pub root: blake3::Hash,
}

/// Master key material (zeroized on drop)
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct MasterKey(pub [u8; KEY_LEN]);

impl MasterKey {
    pub fn new(key: [u8; KEY_LEN]) -> Self {
        Self(key)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

/// Derived encryption key (zeroized on drop)
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct DerivedKey(pub [u8; DERIVED_KEY_LEN]);

impl DerivedKey {
    pub fn as_bytes(&self) -> &[u8; DERIVED_KEY_LEN] {
        &self.0
    }
}

/// Encryption nonce
#[derive(Debug, Clone)]
pub struct Nonce(pub [u8; NONCE_LEN]);

impl Nonce {
    pub fn random() -> Self {
        let mut nonce = [0u8; NONCE_LEN];
        rand::thread_rng().fill(&mut nonce[..]);
        Self(nonce)
    }

    pub fn as_bytes(&self) -> &[u8; NONCE_LEN] {
        &self.0
    }
}
