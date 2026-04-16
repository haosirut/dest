//! GossipSub message broadcasting for ledger sync, chunk announcements, etc.

use crate::message::{GossipMessage, P2pMessage};
use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};
use tracing::debug;

/// Message priority levels for weighted scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl std::fmt::Display for MessagePriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessagePriority::Low => write!(f, "Low"),
            MessagePriority::Normal => write!(f, "Normal"),
            MessagePriority::High => write!(f, "High"),
            MessagePriority::Critical => write!(f, "Critical"),
        }
    }
}

/// GossipSub message queue with priority support.
///
/// Messages are stored in per-priority queues and dequeued in priority order
/// (Critical > High > Normal > Low). Within the same priority, FIFO ordering
/// is maintained.
pub struct GossipQueue {
    queues: HashMap<MessagePriority, VecDeque<GossipMessage>>,
    max_queue_size: usize,
    sequence_counter: u64,
}

impl GossipQueue {
    /// Create a new gossip queue with the given maximum capacity.
    pub fn new(max_queue_size: usize) -> Self {
        let mut queues = HashMap::new();
        queues.insert(MessagePriority::Critical, VecDeque::new());
        queues.insert(MessagePriority::High, VecDeque::new());
        queues.insert(MessagePriority::Normal, VecDeque::new());
        queues.insert(MessagePriority::Low, VecDeque::new());
        Self {
            queues,
            max_queue_size,
            sequence_counter: 0,
        }
    }

    /// Enqueue a message with the given priority.
    pub fn enqueue(
        &mut self,
        priority: MessagePriority,
        peer_id: &str,
        message: P2pMessage,
    ) -> Result<()> {
        let total: usize = self.queues.values().map(|q| q.len()).sum();
        if total >= self.max_queue_size {
            anyhow::bail!("Gossip queue full ({} messages)", total);
        }

        let gossip_msg = GossipMessage {
            sender_peer_id: peer_id.to_string(),
            message,
            timestamp: chrono::Utc::now().timestamp() as u64,
            sequence: self.sequence_counter,
        };
        self.sequence_counter += 1;

        self.queues.get_mut(&priority).unwrap().push_back(gossip_msg);
        debug!(
            "Enqueued gossip message (priority={}, seq={})",
            priority, self.sequence_counter - 1
        );
        Ok(())
    }

    /// Dequeue the next message (highest priority first, FIFO within same priority).
    pub fn dequeue(&mut self) -> Option<GossipMessage> {
        for priority in &[
            MessagePriority::Critical,
            MessagePriority::High,
            MessagePriority::Normal,
            MessagePriority::Low,
        ] {
            if let Some(msg) = self.queues.get_mut(priority).and_then(|q| q.pop_front()) {
                return Some(msg);
            }
        }
        None
    }

    /// Get total number of messages across all priority queues.
    pub fn len(&self) -> usize {
        self.queues.values().map(|q| q.len()).sum()
    }

    /// Check if all queues are empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the number of messages in a specific priority queue.
    pub fn priority_len(&self, priority: MessagePriority) -> usize {
        self.queues.get(&priority).map(|q| q.len()).unwrap_or(0)
    }

    /// Peek at the next message without removing it.
    pub fn peek(&self) -> Option<&GossipMessage> {
        for priority in &[
            MessagePriority::Critical,
            MessagePriority::High,
            MessagePriority::Normal,
            MessagePriority::Low,
        ] {
            if let Some(msg) = self.queues.get(priority).and_then(|q| q.front()) {
                return Some(msg);
            }
        }
        None
    }
}

/// Tracks seen messages to prevent duplicate processing.
///
/// Uses a bounded HashSet that evicts old entries when capacity is reached.
pub struct SeenMessages {
    seen: HashSet<u64>,
    max_size: usize,
}

impl SeenMessages {
    pub fn new(max_size: usize) -> Self {
        Self {
            seen: HashSet::new(),
            max_size,
        }
    }

    /// Check if message was already seen. If not, mark it as seen.
    /// Returns true if the message is new (not seen before).
    pub fn check_and_mark(&mut self, seq: u64) -> bool {
        if self.seen.contains(&seq) {
            return false;
        }
        if self.seen.len() >= self.max_size {
            // Evict oldest entries (simplified: clear first half)
            let to_remove = self.max_size / 2;
            let mut count = 0;
            self.seen.retain(|_| {
                count += 1;
                count > to_remove
            });
        }
        self.seen.insert(seq);
        true
    }

    /// Check if a sequence number has been seen without marking it.
    pub fn contains(&self, seq: u64) -> bool {
        self.seen.contains(&seq)
    }

    /// Get the number of seen message entries.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    /// Clear all seen message entries.
    pub fn clear(&mut self) {
        self.seen.clear();
    }
}

/// Determine message priority based on message type and subscription weight.
///
/// Subscription weight ranges:
/// - 1: Free/archive tier
/// - 2: Standard tier
/// - 3: Premium tier
pub fn get_message_priority(message: &P2pMessage, subscription_weight: u8) -> MessagePriority {
    match message {
        P2pMessage::HeartbeatAck { .. } => MessagePriority::Low,
        P2pMessage::Heartbeat { .. } | P2pMessage::NodeJoin { .. } => MessagePriority::Normal,
        P2pMessage::ChunkAnnounce { .. } | P2pMessage::ChunkRequest { .. } => {
            if subscription_weight >= 3 {
                MessagePriority::High
            } else {
                MessagePriority::Normal
            }
        }
        P2pMessage::LedgerSync { .. } | P2pMessage::LedgerRequest { .. } => MessagePriority::High,
        P2pMessage::ChallengeRequest { .. } | P2pMessage::ChallengeResponse { .. } => {
            MessagePriority::High
        }
        P2pMessage::ReplicationRequest { .. } => {
            if subscription_weight >= 3 {
                MessagePriority::Critical
            } else {
                MessagePriority::High
            }
        }
        P2pMessage::ChunkResponse { .. } => {
            if subscription_weight >= 2 {
                MessagePriority::High
            } else {
                MessagePriority::Normal
            }
        }
        P2pMessage::HostAvailable { .. } => MessagePriority::High,
    }
}

/// Determine subscription weight from tier name.
pub fn subscription_weight_from_tier(tier: &str) -> u8 {
    match tier.to_lowercase().as_str() {
        "premium" => 3,
        "standard" => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gossip_queue_priority_ordering() {
        let mut queue = GossipQueue::new(100);
        queue
            .enqueue(
                MessagePriority::Low,
                "p1",
                P2pMessage::NodeJoin {
                    node_id: "n1".into(),
                    peer_id: "p1".into(),
                    available_space: 100,
                },
            )
            .unwrap();
        queue
            .enqueue(
                MessagePriority::Critical,
                "p2",
                P2pMessage::ReplicationRequest {
                    chunk_id: vaultkeeper_core::types::ChunkId::new(b"test"),
                    target_shards: vec![0],
                },
            )
            .unwrap();

        let first = queue.dequeue().unwrap();
        match &first.message {
            P2pMessage::ReplicationRequest { .. } => {}
            _ => panic!("Expected critical priority message first"),
        }

        // Second should be the low-priority one
        let second = queue.dequeue().unwrap();
        assert_eq!(second.sender_peer_id, "p1");
    }

    #[test]
    fn test_gossip_queue_fifo_same_priority() {
        let mut queue = GossipQueue::new(100);
        queue
            .enqueue(
                MessagePriority::Normal,
                "p1",
                P2pMessage::NodeJoin {
                    node_id: "n1".into(),
                    peer_id: "p1".into(),
                    available_space: 100,
                },
            )
            .unwrap();
        queue
            .enqueue(
                MessagePriority::Normal,
                "p2",
                P2pMessage::NodeJoin {
                    node_id: "n2".into(),
                    peer_id: "p2".into(),
                    available_space: 200,
                },
            )
            .unwrap();

        let first = queue.dequeue().unwrap();
        assert_eq!(first.sender_peer_id, "p1");
        let second = queue.dequeue().unwrap();
        assert_eq!(second.sender_peer_id, "p2");
    }

    #[test]
    fn test_gossip_queue_full() {
        let mut queue = GossipQueue::new(2);
        queue
            .enqueue(
                MessagePriority::Normal,
                "p1",
                P2pMessage::NodeJoin {
                    node_id: "n1".into(),
                    peer_id: "p1".into(),
                    available_space: 100,
                },
            )
            .unwrap();
        queue
            .enqueue(
                MessagePriority::Normal,
                "p2",
                P2pMessage::NodeJoin {
                    node_id: "n2".into(),
                    peer_id: "p2".into(),
                    available_space: 200,
                },
            )
            .unwrap();
        let result = queue.enqueue(
            MessagePriority::Normal,
            "p3",
            P2pMessage::NodeJoin {
                node_id: "n3".into(),
                peer_id: "p3".into(),
                available_space: 300,
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_seen_messages_dedup() {
        let mut seen = SeenMessages::new(100);
        assert!(seen.check_and_mark(1));
        assert!(!seen.check_and_mark(1));
        assert!(seen.check_and_mark(2));
        assert!(seen.contains(1));
        assert!(seen.contains(2));
        assert!(!seen.contains(3));
    }

    #[test]
    fn test_seen_messages_eviction() {
        let mut seen = SeenMessages::new(10);
        for i in 0..10 {
            assert!(seen.check_and_mark(i));
        }
        // Queue is full. Adding one more should trigger eviction.
        assert!(seen.check_and_mark(100));
        // After eviction of first 5, 0..5 may or may not be present depending on eviction order.
        // But 100 must be present.
        assert!(seen.contains(100));
    }

    #[test]
    fn test_message_priority_by_tier() {
        let msg = P2pMessage::ReplicationRequest {
            chunk_id: vaultkeeper_core::types::ChunkId::new(b"test"),
            target_shards: vec![0],
        };
        let weight_free = subscription_weight_from_tier("archive");
        let weight_premium = subscription_weight_from_tier("premium");

        let priority_free = get_message_priority(&msg, weight_free);
        let priority_premium = get_message_priority(&msg, weight_premium);

        assert!(priority_premium > priority_free);
        assert_eq!(priority_free, MessagePriority::High);
        assert_eq!(priority_premium, MessagePriority::Critical);
    }

    #[test]
    fn test_subscription_weight() {
        assert_eq!(subscription_weight_from_tier("premium"), 3);
        assert_eq!(subscription_weight_from_tier("Premium"), 3); // case insensitive
        assert_eq!(subscription_weight_from_tier("standard"), 2);
        assert_eq!(subscription_weight_from_tier("Standard"), 2); // case insensitive
        assert_eq!(subscription_weight_from_tier("archive"), 1);
        assert_eq!(subscription_weight_from_tier("free"), 1);
        assert_eq!(subscription_weight_from_tier("unknown"), 1);
    }

    #[test]
    fn test_host_available_priority() {
        let msg = P2pMessage::HostAvailable {
            node_id: "host1".to_string(),
            peer_id: "p1".to_string(),
            available_space: 100,
            last_seen_timestamp: 0,
        };
        let priority = get_message_priority(&msg, 1);
        assert_eq!(priority, MessagePriority::High);
    }

    #[test]
    fn test_queue_len_and_is_empty() {
        let queue = GossipQueue::new(100);
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_queue_priority_len() {
        let mut queue = GossipQueue::new(100);
        queue
            .enqueue(
                MessagePriority::Critical,
                "p1",
                P2pMessage::NodeJoin {
                    node_id: "n1".into(),
                    peer_id: "p1".into(),
                    available_space: 100,
                },
            )
            .unwrap();
        queue
            .enqueue(
                MessagePriority::Low,
                "p2",
                P2pMessage::NodeJoin {
                    node_id: "n2".into(),
                    peer_id: "p2".into(),
                    available_space: 200,
                },
            )
            .unwrap();
        assert_eq!(queue.priority_len(MessagePriority::Critical), 1);
        assert_eq!(queue.priority_len(MessagePriority::Low), 1);
        assert_eq!(queue.priority_len(MessagePriority::High), 0);
    }

    #[test]
    fn test_message_priority_display() {
        assert_eq!(MessagePriority::Low.to_string(), "Low");
        assert_eq!(MessagePriority::Critical.to_string(), "Critical");
    }
}
