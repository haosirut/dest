//! P2P node: main entry point for the VaultKeeper networking layer.
//!
//! This module provides the `P2PNode` struct, the primary interface for
//! Tauri and CLI consumers. The node manages identity, heartbeat tracking,
//! gossip message queuing, and simulated file upload/download operations.
//!
//! Because a real libp2p Swarm requires a live async runtime and network
//! I/O, the networking layer is simulated: state is held in-memory and
//! methods perform logical operations without opening actual connections.

use crate::{
    behaviour::VaultKeeperBehaviour,
    config::P2pConfig,
    gossip::GossipQueue,
    heartbeat::{HeartbeatConfig, HeartbeatManager},
    message::P2pMessage,
    transport::generate_identity,
};
use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::info;

// ---------------------------------------------------------------------------
// DiskType
// ---------------------------------------------------------------------------

/// Storage disk type used when uploading files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskType {
    Hdd,
    Ssd,
    Nvme,
}

impl std::fmt::Display for DiskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiskType::Hdd => write!(f, "hdd"),
            DiskType::Ssd => write!(f, "ssd"),
            DiskType::Nvme => write!(f, "nvme"),
        }
    }
}

impl std::str::FromStr for DiskType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hdd" => Ok(DiskType::Hdd),
            "ssd" => Ok(DiskType::Ssd),
            "nvme" => Ok(DiskType::Nvme),
            other => anyhow::bail!("Unknown disk type '{}', expected hdd/ssd/nvme", other),
        }
    }
}

// ---------------------------------------------------------------------------
// UploadParams
// ---------------------------------------------------------------------------

/// Parameters for an upload request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadParams {
    /// Number of replicas to distribute across the network (1-255).
    pub replication: u8,
    /// Preferred disk type for storage hosts.
    pub disk_type: DiskType,
    /// Enable cushion padding for extra data protection.
    pub cushion_enabled: bool,
    /// Maximum cost in decimal currency units.
    pub max_cost: Decimal,
}

impl Default for UploadParams {
    fn default() -> Self {
        Self {
            replication: 3,
            disk_type: DiskType::Ssd,
            cushion_enabled: true,
            max_cost: Decimal::ZERO,
        }
    }
}

// ---------------------------------------------------------------------------
// FileMetadata
// ---------------------------------------------------------------------------

/// Metadata describing a stored file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Unique file identifier (blake3 hash hex).
    pub file_id: String,
    /// Original file name (or empty if unnamed).
    pub name: String,
    /// File size in bytes.
    pub size: u64,
    /// Chunk identifiers produced during upload.
    pub chunks: Vec<String>,
    /// ISO 8601 timestamp of upload.
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// P2PNode
// ---------------------------------------------------------------------------

/// The main P2P node handle.
///
/// Stores node identity, configuration, and subsystem handles for heartbeat,
/// gossip, and behaviour tracking. File data is held in-memory for the
/// simulated layer.
pub struct P2PNode {
    /// Peer ID string (base58-encoded multihash).
    pub peer_id: String,
    #[allow(dead_code)]
    config: P2pConfig,
    #[allow(dead_code)]
    heartbeat: Arc<parking_lot::RwLock<HeartbeatManager>>,
    gossip_queue: Arc<parking_lot::RwLock<GossipQueue>>,
    behaviour: Arc<parking_lot::RwLock<VaultKeeperBehaviour>>,
    stored_files: Arc<parking_lot::RwLock<HashMap<String, Vec<u8>>>>,
    file_metadata: Arc<parking_lot::RwLock<HashMap<String, FileMetadata>>>,
    #[allow(dead_code)]
    identity: libp2p::identity::Keypair,
    available_space: Arc<AtomicU64>,
}

impl P2PNode {
    /// Create a new P2P node with the given configuration.
    ///
    /// Generates a fresh Ed25519 identity keypair and initializes all
    /// subsystems (heartbeat, gossip queue, behaviour flags).
    pub async fn new(config: P2pConfig) -> Result<Self> {
        let identity = generate_identity();
        let peer_id = identity.public().to_peer_id().to_string();

        info!("Initializing P2P node: {}", peer_id);

        let heartbeat = Arc::new(parking_lot::RwLock::new(HeartbeatManager::new(
            peer_id.clone(),
            HeartbeatConfig {
                interval_secs: config.heartbeat_interval_secs,
                ..Default::default()
            },
        )));

        let gossip_queue = Arc::new(parking_lot::RwLock::new(GossipQueue::new(10_000)));

        let mut behaviour = VaultKeeperBehaviour::new();
        if config.enable_mdns {
            behaviour.mark_mdns_enabled();
        }
        let behaviour = Arc::new(parking_lot::RwLock::new(behaviour));

        Ok(Self {
            peer_id,
            config,
            heartbeat,
            gossip_queue,
            behaviour,
            stored_files: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            file_metadata: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            identity,
            available_space: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Create a new P2P node with NAT traversal features enabled.
    ///
    /// Convenience constructor that enables autonat, relay, and dcutr.
    pub async fn new_with_nat_support() -> Result<Self> {
        let config = P2pConfig {
            enable_nat: true,
            enable_relay: true,
            enable_mdns: true,
            ..P2pConfig::default()
        };
        let node = Self::new(config).await?;
        {
            let mut behaviour = node.behaviour.write();
            *behaviour = VaultKeeperBehaviour::new_with_nat();
        }
        info!("P2P node created with NAT traversal support");
        Ok(node)
    }

    /// Get the node's peer ID as a string.
    pub fn peer_id_str(&self) -> String {
        self.peer_id.clone()
    }

    /// Upload file data as chunks and distribute (simulated).
    ///
    /// Chunks the input data, generates chunk IDs via blake3, stores the
    /// raw data locally, and returns a unique `file_id`.
    pub async fn upload_chunks(
        &mut self,
        data: &[u8],
        params: UploadParams,
    ) -> Result<String> {
        if data.is_empty() {
            anyhow::bail!("Cannot upload empty data");
        }

        // Chunk the data using core chunking module
        let chunks = vaultkeeper_core::chunking::chunk_data(data);
        let chunk_ids: Vec<String> = vaultkeeper_core::chunking::generate_chunk_ids(&chunks)
            .iter()
            .map(|id| id.to_string())
            .collect();

        // Generate file_id from full data hash
        let file_id = blake3::hash(data).to_hex().to_string();

        let timestamp = chrono::Utc::now().to_rfc3339();

        // Build file metadata
        let metadata = FileMetadata {
            file_id: file_id.clone(),
            name: String::new(),
            size: data.len() as u64,
            chunks: chunk_ids.clone(),
            timestamp: timestamp.clone(),
        };

        // Store data and metadata
        {
            let mut files = self.stored_files.write();
            files.insert(file_id.clone(), data.to_vec());
        }
        {
            let mut meta_map = self.file_metadata.write();
            meta_map.insert(file_id.clone(), metadata);
        }

        info!(
            "Uploaded {} bytes ({} chunks, replication={}, disk={}) -> file_id={}",
            data.len(),
            chunks.len(),
            params.replication,
            params.disk_type,
            file_id,
        );

        Ok(file_id)
    }

    /// Download a file by its file_id.
    ///
    /// Looks up the stored data and returns it. In a production
    /// implementation this would fetch shards from peers, reassemble,
    /// and decrypt.
    pub async fn download_file(&mut self, file_id: &str) -> Result<Vec<u8>> {
        let files = self.stored_files.read();
        let data = files
            .get(file_id)
            .cloned()
            .with_context(|| format!("File '{}' not found in local store", file_id))?;

        info!("Downloaded file {} ({} bytes)", file_id, data.len());
        Ok(data)
    }

    /// Send a raw gossip message to a named topic.
    ///
    /// The message bytes are wrapped in an internal queue entry. In a
    /// production implementation this would publish to the GossipSub mesh.
    pub async fn send_gossip(&self, topic: &str, message: Vec<u8>) -> Result<()> {
        let mut queue = self.gossip_queue.write();

        // Wrap raw topic + message into an internal P2pMessage for the queue
        let wrapped = P2pMessage::LedgerSync {
            merkle_root: topic.to_string(),
            from_seq: 0,
            entries: vec![message],
        };

        let priority = crate::gossip::get_message_priority(&wrapped, 2);
        queue.enqueue(priority, &self.peer_id, wrapped)?;
        info!("Gossip message enqueued on topic: {}", topic);
        Ok(())
    }

    /// Get a list of connected peer IDs.
    ///
    /// Returns peer IDs tracked by the heartbeat manager. In a production
    /// implementation this would query the Kademlia routing table and active
    /// connections.
    pub async fn get_peers(&self) -> Vec<String> {
        let hb = self.heartbeat.read();
        hb.peer_ids()
    }

    /// Update the available storage space for this node.
    ///
    /// The value is stored atomically and included in heartbeat messages
    /// sent to peers.
    pub fn update_available_space(&self, space: u64) {
        self.available_space.store(space, Ordering::Relaxed);
        let mut hb = self.heartbeat.write();
        hb.set_available_space(space);
        info!("Updated available space: {} bytes", space);
    }

    /// Initialize all P2P subsystems (Kademlia, GossipSub, Heartbeat).
    pub async fn initialize(&self) -> Result<()> {
        let mut behaviour = self.behaviour.write();
        behaviour.mark_kademlia_ready();
        behaviour.mark_gossip_ready();
        behaviour.mark_heartbeat_active();
        info!("P2P subsystems initialized");
        Ok(())
    }

    /// Check if the node has NAT traversal enabled.
    pub fn has_nat_support(&self) -> bool {
        self.behaviour.read().has_nat_support()
    }

    /// Get the current available space in bytes.
    pub fn get_available_space(&self) -> u64 {
        self.available_space.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::P2pConfig;

    #[tokio::test]
    async fn test_create_node() {
        let config = P2pConfig::default();
        let node = P2PNode::new(config).await.unwrap();
        assert!(!node.peer_id_str().is_empty());
        // Peer ID should be a valid base58 string (starts with 'Qm' or '12')
        assert!(node.peer_id_str().len() > 10);
    }

    #[tokio::test]
    async fn test_create_node_with_nat() {
        let node = P2PNode::new_with_nat_support().await.unwrap();
        assert!(!node.peer_id_str().is_empty());
        assert!(node.has_nat_support());
    }

    #[tokio::test]
    async fn test_upload_and_download_roundtrip() {
        let config = P2pConfig::default();
        let mut node = P2PNode::new(config).await.unwrap();

        let data = b"Hello, VaultKeeper P2P storage!";
        let params = UploadParams::default();
        let file_id = node.upload_chunks(data, params).await.unwrap();

        let retrieved = node.download_file(&file_id).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_upload_empty_data_fails() {
        let config = P2pConfig::default();
        let mut node = P2PNode::new(config).await.unwrap();

        let params = UploadParams::default();
        let result = node.upload_chunks(&[], params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_download_nonexistent_file() {
        let config = P2pConfig::default();
        let mut node = P2PNode::new(config).await.unwrap();

        let result = node.download_file("nonexistent_file_id").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_upload_large_data() {
        let config = P2pConfig::default();
        let mut node = P2PNode::new(config).await.unwrap();

        // 10 MB of data
        let data = vec![0xABu8; 10 * 1024 * 1024];
        let params = UploadParams {
            replication: 2,
            disk_type: DiskType::Nvme,
            cushion_enabled: false,
            max_cost: Decimal::new(100, 0),
        };
        let file_id = node.upload_chunks(&data, params).await.unwrap();

        let retrieved = node.download_file(&file_id).await.unwrap();
        assert_eq!(retrieved.len(), data.len());
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_send_gossip() {
        let config = P2pConfig::default();
        let node = P2PNode::new(config).await.unwrap();
        let result = node
            .send_gossip("test-topic", b"hello peers".to_vec())
            .await;
        assert!(result.is_ok());

        // Verify the message was enqueued
        let queue = node.gossip_queue.read();
        assert_eq!(queue.len(), 1);
    }

    #[tokio::test]
    async fn test_get_peers_empty() {
        let config = P2pConfig::default();
        let node = P2PNode::new(config).await.unwrap();
        let peers = node.get_peers().await;
        assert!(peers.is_empty());
    }

    #[tokio::test]
    async fn test_update_available_space() {
        let config = P2pConfig::default();
        let node = P2PNode::new(config).await.unwrap();
        node.update_available_space(1024 * 1024 * 1024);
        assert_eq!(node.get_available_space(), 1024 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_initialize() {
        let config = P2pConfig::default();
        let node = P2PNode::new(config).await.unwrap();
        node.initialize().await.unwrap();
        assert!(node.behaviour.read().is_fully_initialized());
    }

    #[test]
    fn test_disk_type_from_str() {
        assert_eq!("hdd".parse::<DiskType>().unwrap(), DiskType::Hdd);
        assert_eq!("SSD".parse::<DiskType>().unwrap(), DiskType::Ssd);
        assert_eq!("Nvme".parse::<DiskType>().unwrap(), DiskType::Nvme);
        assert!("unknown".parse::<DiskType>().is_err());
    }

    #[test]
    fn test_disk_type_display() {
        assert_eq!(DiskType::Hdd.to_string(), "hdd");
        assert_eq!(DiskType::Ssd.to_string(), "ssd");
        assert_eq!(DiskType::Nvme.to_string(), "nvme");
    }

    #[test]
    fn test_upload_params_default() {
        let params = UploadParams::default();
        assert_eq!(params.replication, 3);
        assert_eq!(params.disk_type, DiskType::Ssd);
        assert!(params.cushion_enabled);
        assert_eq!(params.max_cost, Decimal::ZERO);
    }

    #[test]
    fn test_file_metadata_serialization() {
        let meta = FileMetadata {
            file_id: "abc123".to_string(),
            name: "test.txt".to_string(),
            size: 1024,
            chunks: vec!["chunk1".to_string(), "chunk2".to_string()],
            timestamp: "2024-01-01T00:00:00+00:00".to_string(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: FileMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.file_id, "abc123");
        assert_eq!(deserialized.size, 1024);
        assert_eq!(deserialized.chunks.len(), 2);
    }

    #[tokio::test]
    async fn test_upload_preserves_metadata() {
        let config = P2pConfig::default();
        let mut node = P2PNode::new(config).await.unwrap();

        let data = b"test file content";
        let params = UploadParams::default();
        let file_id = node.upload_chunks(data, params).await.unwrap();

        let meta_map = node.file_metadata.read();
        let meta = meta_map.get(&file_id).unwrap();
        assert_eq!(meta.size, data.len() as u64);
        assert!(!meta.chunks.is_empty());
        assert!(!meta.timestamp.is_empty());
    }
}
