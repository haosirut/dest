//! Network message types for P2P communication.

use vaultkeeper_core::types::{StorageChallenge, StorageProof};
use serde::{Deserialize, Serialize};
use vaultkeeper_core::ChunkId;

/// Topic names for GossipSub
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GossipTopic {
    /// Ledger state updates
    Ledger,
    /// Chunk availability announcements
    ChunkAvailability,
    /// Node metadata updates
    NodeInfo,
    /// Replication requests
    Replication,
    /// Billing updates
    Billing,
}

impl GossipTopic {
    pub fn topic_name(&self, prefix: &str) -> String {
        match self {
            GossipTopic::Ledger => format!("{}/ledger", prefix),
            GossipTopic::ChunkAvailability => format!("{}/chunks", prefix),
            GossipTopic::NodeInfo => format!("{}/node-info", prefix),
            GossipTopic::Replication => format!("{}/replication", prefix),
            GossipTopic::Billing => format!("{}/billing", prefix),
        }
    }
}

/// All P2P message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2pMessage {
    /// Request chunks from a peer
    ChunkRequest {
        chunk_id: ChunkId,
        shard_indices: Vec<usize>,
    },
    /// Respond with chunk shards
    ChunkResponse {
        chunk_id: ChunkId,
        shard_index: usize,
        data: Vec<u8>,
    },
    /// Announce chunk availability
    ChunkAnnounce {
        chunk_id: ChunkId,
        available_shards: Vec<usize>,
    },
    /// Request proof of storage
    ChallengeRequest(StorageChallenge),
    /// Respond with storage proof
    ChallengeResponse(StorageProof),
    /// Heartbeat ping
    Heartbeat {
        node_id: String,
        timestamp: u64,
        available_space_bytes: u64,
    },
    /// Heartbeat pong
    HeartbeatAck {
        node_id: String,
        timestamp: u64,
    },
    /// Ledger sync message
    LedgerSync {
        merkle_root: String,
        from_seq: u64,
        entries: Vec<Vec<u8>>,
    },
    /// Request ledger entries from a specific sequence number
    LedgerRequest {
        from_seq: u64,
    },
    /// Replication request
    ReplicationRequest {
        chunk_id: ChunkId,
        target_shards: Vec<usize>,
    },
    /// Node joining the network
    NodeJoin {
        node_id: String,
        peer_id: String,
        available_space: u64,
    },
}

/// Wrapper for gossip messages with sender info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipMessage {
    pub sender_peer_id: String,
    pub message: P2pMessage,
    pub timestamp: u64,
    pub sequence: u64,
}
