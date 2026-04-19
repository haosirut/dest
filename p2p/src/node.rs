//! P2P node: main entry point for the networking layer.

use crate::{
    behaviour::{VaultKeeperBehaviour, generate_identity},
    config::P2pConfig,
    gossip::GossipQueue,
    heartbeat::{HeartbeatConfig, HeartbeatManager},
    message::{GossipTopic, P2pMessage},
};
use anyhow::Result;
use libp2p::{PeerId, identity::Keypair};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// P2P node handle
pub struct P2pNode {
    pub peer_id: PeerId,
    pub config: P2pConfig,
    pub heartbeat: Arc<RwLock<HeartbeatManager>>,
    pub gossip_queue: Arc<RwLock<GossipQueue>>,
    pub behaviour: Arc<RwLock<VaultKeeperBehaviour>>,
    _transport_keypair: Keypair,
}

impl P2pNode {
    /// Create a new P2P node with the given configuration.
    pub async fn new(config: P2pConfig) -> Result<Self> {
        let keypair = generate_identity();
        let peer_id = keypair.public().to_peer_id();

        info!("Initializing P2P node: {}", peer_id);

        let heartbeat = Arc::new(RwLock::new(HeartbeatManager::new(
            peer_id.to_string(),
            HeartbeatConfig {
                interval_secs: config.heartbeat_interval_secs,
                ..Default::default()
            },
        )));

        let gossip_queue = Arc::new(RwLock::new(GossipQueue::new(10_000)));
        let behaviour = Arc::new(RwLock::new(VaultKeeperBehaviour::new()));

        Ok(Self {
            peer_id,
            config,
            heartbeat,
            gossip_queue,
            behaviour,
            _transport_keypair: keypair,
        })
    }

    /// Send a message through gossip
    pub async fn send_gossip(&self, topic: GossipTopic, message: P2pMessage) -> Result<()> {
        let mut queue = self.gossip_queue.write().await;
        let priority = crate::gossip::get_message_priority(&message, 1);
        queue.enqueue(priority, &self.peer_id.to_string(), message)?;
        info!("Enqueued gossip message on topic: {:?}", topic);
        Ok(())
    }

    /// Get the node's peer ID as a string
    pub fn peer_id_str(&self) -> String {
        self.peer_id.to_string()
    }

    /// Update available space for heartbeats
    pub async fn update_available_space(&self, space: u64) {
        let mut hb = self.heartbeat.write().await;
        hb.set_available_space(space);
    }

    /// Initialize all P2P subsystems
    pub async fn initialize(&self) -> Result<()> {
        let mut behaviour = self.behaviour.write().await;
        behaviour.mark_kademlia_ready();
        behaviour.mark_gossip_ready();
        behaviour.mark_heartbeat_active();
        info!("P2P subsystems initialized");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_node() {
        let config = P2pConfig::default();
        let node = P2pNode::new(config).await.unwrap();
        assert!(!node.peer_id_str().is_empty());
    }

    #[tokio::test]
    async fn test_send_gossip() {
        let config = P2pConfig::default();
        let node = P2pNode::new(config).await.unwrap();
        let msg = P2pMessage::NodeJoin {
            node_id: "test_node".to_string(),
            peer_id: node.peer_id_str(),
            available_space: 1024,
        };
        let result = node.send_gossip(GossipTopic::NodeInfo, msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_initialize() {
        let config = P2pConfig::default();
        let node = P2pNode::new(config).await.unwrap();
        node.initialize().await.unwrap();
        let behaviour = node.behaviour.read().await;
        assert!(behaviour.is_fully_initialized());
    }

    #[tokio::test]
    async fn test_update_available_space() {
        let config = P2pConfig::default();
        let node = P2pNode::new(config).await.unwrap();
        node.update_available_space(1024 * 1024).await;
        let hb = node.heartbeat.read().await;
        assert_eq!(hb.available_space, 1024 * 1024);
    }
}
