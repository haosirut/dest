use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const CHUNK_SIZE: usize = 4 * 1024 * 1024;
pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 24;
pub const TAG_LEN: usize = 16;
pub const SALT_LEN: usize = 32;
pub const RS_DATA_SHARDS: usize = 3;
pub const RS_PARITY_SHARDS: usize = 2;
pub const RS_TOTAL_SHARDS: usize = RS_DATA_SHARDS + RS_PARITY_SHARDS;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkId(pub [u8; 32]);

impl ChunkId {
    pub fn new(data: &[u8]) -> Self {
        Self(blake3::hash(data).into())
    }
    pub fn as_hex(&self) -> String { hex::encode(self.0) }
    pub fn from_hex(s: &str) -> anyhow::Result<Self> {
        let b = hex::decode(s)?;
        if b.len() != 32 { anyhow::bail!("ChunkId must be 32 bytes"); }
        let mut id = [0u8; 32]; id.copy_from_slice(&b); Ok(Self(id))
    }
}

impl std::fmt::Display for ChunkId { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.as_hex()) } }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardIndex(pub usize);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedShard {
    pub chunk_id: ChunkId,
    pub shard_index: ShardIndex,
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageChallenge {
    pub chunk_id: ChunkId,
    pub leaf_indices: Vec<u64>,
    pub nonce: [u8; NONCE_LEN],
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageProof {
    pub chunk_id: ChunkId,
    pub leaf_indices: Vec<u64>,
    pub leaves: Vec<Vec<u8>>,
    pub root_hash: [u8; 32],
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct DerivedKey(pub [u8; KEY_LEN]);

impl DerivedKey {
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] { &self.0 }
    pub fn from_slice(s: &[u8]) -> Self {
        let mut k = [0u8; KEY_LEN]; k.copy_from_slice(&s[..KEY_LEN]); Self(k)
    }
}
