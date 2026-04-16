//! P2P network configuration.

use serde::{Deserialize, Serialize};

/// Default listen port for P2P
pub const DEFAULT_LISTEN_PORT: u16 = 9444;

/// Bootstrap nodes — public libp2p bootstrap servers for initial peer discovery.
/// Replace with your own VPS-hosted nodes for production.
pub const DEFAULT_BOOTSTRAP_NODES: &[&str] = &[
    "/dns4/bootstrap.libp2p.io/tcp/443/wss/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "/dns4/bootstrap.libp2p.io/tcp/443/wss/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbzCUcVvMavz",
    "/dns4/bootstrap.libp2p.io/tcp/443/wss/p2p/QmZa1sAxajnQjVM8WjWXoMbmPd7NsHf6hnvAakf9B7JqMN",
];

/// Default heartbeat interval in seconds (15 minutes)
pub const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 900;

/// P2P network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pConfig {
    /// Local address to listen on (e.g. "0.0.0.0:9444")
    pub listen_addr: String,
    /// Bootstrap node multiaddresses
    pub bootstrap_nodes: Vec<String>,
    /// Heartbeat interval in seconds
    pub heartbeat_interval_secs: u64,
    /// Enable NAT traversal (autonat, relay, dcutr)
    pub enable_nat: bool,
    /// Enable relay server/client
    pub enable_relay: bool,
    /// Enable mDNS local discovery
    pub enable_mdns: bool,
    /// DHT protocol name
    pub dht_protocol: String,
    /// GossipSub topic prefix
    pub gossip_topic_prefix: String,
    /// Maximum simultaneous connections
    pub max_connections: usize,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            listen_addr: format!("0.0.0.0:{}", DEFAULT_LISTEN_PORT),
            bootstrap_nodes: DEFAULT_BOOTSTRAP_NODES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            heartbeat_interval_secs: DEFAULT_HEARTBEAT_INTERVAL_SECS,
            enable_nat: true,
            enable_relay: true,
            enable_mdns: true,
            dht_protocol: "/vaultkeeper/dht/1.0.0".to_string(),
            gossip_topic_prefix: "vaultkeeper".to_string(),
            max_connections: 50,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_listen_addr() {
        let config = P2pConfig::default();
        assert_eq!(config.listen_addr, "0.0.0.0:9444");
    }

    #[test]
    fn test_default_config_bootstrap_nodes() {
        let config = P2pConfig::default();
        assert_eq!(config.bootstrap_nodes.len(), 3);
        assert!(config.bootstrap_nodes[0].contains("/dns4/bootstrap.libp2p.io"));
    }

    #[test]
    fn test_default_config_nat_flags() {
        let config = P2pConfig::default();
        assert!(config.enable_nat);
        assert!(config.enable_relay);
        assert!(config.enable_mdns);
    }

    #[test]
    fn test_default_config_heartbeat() {
        let config = P2pConfig::default();
        assert_eq!(config.heartbeat_interval_secs, 900);
    }

    #[test]
    fn test_default_config_serialization_roundtrip() {
        let config = P2pConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: P2pConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.listen_addr, config.listen_addr);
        assert_eq!(deserialized.bootstrap_nodes.len(), config.bootstrap_nodes.len());
    }
}
