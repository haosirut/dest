//! Network message types for P2P communication.

use serde::{Deserialize, Serialize};
use vaultkeeper_core::types::{ChunkId, StorageChallenge, StorageProof};

/// Topic names for GossipSub
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// Host re-availability announcements ("I'm back")
    HostAvailable,
}

impl GossipTopic {
    pub fn topic_name(&self, prefix: &str) -> String {
        match self {
            GossipTopic::Ledger => format!("{}/ledger", prefix),
            GossipTopic::ChunkAvailability => format!("{}/chunks", prefix),
            GossipTopic::NodeInfo => format!("{}/node-info", prefix),
            GossipTopic::Replication => format!("{}/replication", prefix),
            GossipTopic::Billing => format!("{}/billing", prefix),
            GossipTopic::HostAvailable => format!("{}/host-available", prefix),
        }
    }
}

impl std::fmt::Display for GossipTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.topic_name("vaultkeeper"))
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
    /// Host announcing they are back online after downtime
    HostAvailable {
        node_id: String,
        peer_id: String,
        available_space: u64,
        last_seen_timestamp: u64,
    },
}

/// Wrapper for gossip messages with sender info and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipMessage {
    pub sender_peer_id: String,
    pub message: P2pMessage,
    pub timestamp: u64,
    pub sequence: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gossip_topic_names() {
        let topics = vec![
            (GossipTopic::Ledger, "vaultkeeper/ledger"),
            (GossipTopic::ChunkAvailability, "vaultkeeper/chunks"),
            (GossipTopic::NodeInfo, "vaultkeeper/node-info"),
            (GossipTopic::Replication, "vaultkeeper/replication"),
            (GossipTopic::Billing, "vaultkeeper/billing"),
            (GossipTopic::HostAvailable, "vaultkeeper/host-available"),
        ];
        for (topic, expected) in topics {
            assert_eq!(topic.topic_name("vaultkeeper"), expected);
        }
    }

    #[test]
    fn test_gossip_topic_display() {
        assert_eq!(GossipTopic::Ledger.to_string(), "vaultkeeper/ledger");
        assert_eq!(GossipTopic::HostAvailable.to_string(), "vaultkeeper/host-available");
    }

    #[test]
    fn test_message_serialization_roundtrip() {
        let msg = P2pMessage::NodeJoin {
            node_id: "node1".to_string(),
            peer_id: "12D3KooW".to_string(),
            available_space: 1024,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: P2pMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            P2pMessage::NodeJoin { node_id, available_space, .. } => {
                assert_eq!(node_id, "node1");
                assert_eq!(available_space, 1024);
            }
            _ => panic!("Expected NodeJoin"),
        }
    }

    #[test]
    fn test_host_available_message() {
        let msg = P2pMessage::HostAvailable {
            node_id: "host1".to_string(),
            peer_id: "12D3KooW".to_string(),
            available_space: 2048,
            last_seen_timestamp: 1700000000,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: P2pMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            P2pMessage::HostAvailable { node_id, available_space, last_seen_timestamp, .. } => {
                assert_eq!(node_id, "host1");
                assert_eq!(available_space, 2048);
                assert_eq!(last_seen_timestamp, 1700000000);
            }
            _ => panic!("Expected HostAvailable"),
        }
    }

    #[test]
    fn test_gossip_message_wrapper() {
        let inner = P2pMessage::Heartbeat {
            node_id: "node1".to_string(),
            timestamp: 1000,
            available_space_bytes: 512,
        };
        let gossip = GossipMessage {
            sender_peer_id: "12D3KooW".to_string(),
            message: inner,
            timestamp: 1000,
            sequence: 42,
        };
        let json = serde_json::to_string(&gossip).unwrap();
        let deserialized: GossipMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sender_peer_id, "12D3KooW");
        assert_eq!(deserialized.sequence, 42);
    }

    #[test]
    fn test_challenge_message_serialization() {
        let chunk_id = ChunkId::new(b"test_data");
        let challenge = StorageChallenge {
            chunk_id: chunk_id.clone(),
            leaf_indices: vec![0, 1, 2],
            nonce: [0u8; 24],
            timestamp: 1700000000,
        };
        let msg = P2pMessage::ChallengeRequest(challenge);
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: P2pMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            P2pMessage::ChallengeRequest(c) => {
                assert_eq!(c.chunk_id, chunk_id);
                assert_eq!(c.leaf_indices.len(), 3);
            }
            _ => panic!("Expected ChallengeRequest"),
        }
    }
}
