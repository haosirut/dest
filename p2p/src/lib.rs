//! VaultKeeper P2P — libp2p v0.54 networking layer.
//!
//! Features: Kademlia DHT discovery, GossipSub message broadcasting,
//! heartbeat protocol, challenge-response Proof-of-Storage, NAT traversal
//! (relay, autonat, dcutr), and simulated file upload/download.

pub mod behaviour;
pub mod challenge;
pub mod config;
pub mod discovery;
pub mod gossip;
pub mod heartbeat;
pub mod message;
pub mod node;
pub mod transport;

pub use config::P2pConfig;
pub use node::{DiskType, FileMetadata, P2PNode, P2pNode, UploadParams};
