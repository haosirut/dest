//! VaultKeeper P2P — libp2p networking layer.
//!
//! Features: Kademlia DHT discovery, GossipSub message broadcasting,
//! heartbeat protocol, challenge-response Proof-of-Storage, offline ledger sync.

pub mod behaviour;
pub mod challenge;
pub mod config;
pub mod discovery;
pub mod gossip;
pub mod heartbeat;
pub mod message;
pub mod node;

pub use config::P2pConfig;
pub use node::P2pNode;
