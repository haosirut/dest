//! P2P network configuration.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Default listen port for P2P
pub const DEFAULT_LISTEN_PORT: u16 = 9444;

/// Bootstrap nodes (3 hardcoded for initial discovery)
pub const DEFAULT_BOOTSTRAP_NODES: &[&str] = &[
    "/dns4/bootstrap1.vaultkeeper.net/tcp/9444/p2p/QmBootstrap1Placeholder0000000000000000000000000000000",
    "/dns4/bootstrap2.vaultkeeper.net/tcp/9444/p2p/QmBootstrap2Placeholder0000000000000000000000000000000",
    "/dns4/bootstrap3.vaultkeeper.net/tcp/9444/p2p/QmBootstrap3Placeholder0000000000000000000000000000000",
];

/// P2P network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pConfig {
    /// Local address to listen on
    pub listen_addr: SocketAddr,
    /// Bootstrap node multiaddresses
    pub bootstrap_nodes: Vec<String>,
    /// Enable NAT traversal
    pub enable_nat: bool,
    /// DHT protocol name
    pub dht_protocol: String,
    /// GossipSub topic prefix
    pub gossip_topic_prefix: String,
    /// Heartbeat interval in seconds
    pub heartbeat_interval_secs: u64,
    /// Maximum simultaneous connections
    pub max_connections: usize,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            listen_addr: format!("0.0.0.0:{}", DEFAULT_LISTEN_PORT).parse().unwrap(),
            bootstrap_nodes: DEFAULT_BOOTSTRAP_NODES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            enable_nat: true,
            dht_protocol: "/vaultkeeper/dht/1.0.0".to_string(),
            gossip_topic_prefix: "vaultkeeper".to_string(),
            heartbeat_interval_secs: 900, // 15 min
            max_connections: 50,
        }
    }
}
