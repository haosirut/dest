//! Composite network behaviour for the VaultKeeper P2P node.
//!
//! Provides the infrastructure for combining Kademlia DHT, GossipSub,
//! heartbeat monitoring, and NAT traversal protocols. Since
//! `#[derive(NetworkBehaviour)]` requires a full Swarm event loop,
//! this module provides the initialization flags and helpers.

use crate::transport::{build_transport, generate_identity};

/// Composite behaviour flags for the VaultKeeper P2P node.
///
/// Tracks which protocols have been initialized and are active.
/// In a full implementation with a live Swarm, this would be replaced
/// by a `#[derive(NetworkBehaviour)]` struct combining all sub-protocols.
pub struct VaultKeeperBehaviour {
    /// Kademlia DHT for peer discovery
    pub kademlia_initialized: bool,
    /// GossipSub for message broadcasting
    pub gossip_initialized: bool,
    /// Heartbeat monitoring active
    pub heartbeat_active: bool,
    /// NAT traversal (autonat) enabled
    pub nat_traversal_enabled: bool,
    /// Relay protocol enabled
    pub relay_enabled: bool,
    /// DCUtR (direct connection upgrade through relay) enabled
    pub dcutr_enabled: bool,
    /// mDNS local discovery enabled
    pub mdns_enabled: bool,
}

impl VaultKeeperBehaviour {
    /// Create a new behaviour with all protocols disabled.
    pub fn new() -> Self {
        Self {
            kademlia_initialized: false,
            gossip_initialized: false,
            heartbeat_active: false,
            nat_traversal_enabled: false,
            relay_enabled: false,
            dcutr_enabled: false,
            mdns_enabled: false,
        }
    }

    /// Create a behaviour with NAT traversal protocols pre-configured.
    pub fn new_with_nat() -> Self {
        let mut behaviour = Self::new();
        behaviour.nat_traversal_enabled = true;
        behaviour.relay_enabled = true;
        behaviour.dcutr_enabled = true;
        behaviour
    }

    /// Mark Kademlia DHT as ready.
    pub fn mark_kademlia_ready(&mut self) {
        self.kademlia_initialized = true;
    }

    /// Mark GossipSub as ready.
    pub fn mark_gossip_ready(&mut self) {
        self.gossip_initialized = true;
    }

    /// Mark heartbeat protocol as active.
    pub fn mark_heartbeat_active(&mut self) {
        self.heartbeat_active = true;
    }

    /// Mark mDNS as enabled.
    pub fn mark_mdns_enabled(&mut self) {
        self.mdns_enabled = true;
    }

    /// Check if all core protocols are initialized.
    pub fn is_fully_initialized(&self) -> bool {
        self.kademlia_initialized && self.gossip_initialized && self.heartbeat_active
    }

    /// Check if NAT traversal features are enabled.
    pub fn has_nat_support(&self) -> bool {
        self.nat_traversal_enabled || self.relay_enabled || self.dcutr_enabled
    }
}

impl Default for VaultKeeperBehaviour {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a transport using the transport module.
/// Kept here for backward compatibility; delegates to `transport::build_transport`.
pub fn build_behaviour_transport(
    keypair: &libp2p::identity::Keypair,
) -> anyhow::Result<crate::transport::BoxedTransport> {
    build_transport(keypair)
}

/// Generate a new identity using the transport module.
pub fn build_behaviour_identity() -> libp2p::identity::Keypair {
    generate_identity()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_behaviour_default() {
        let behaviour = VaultKeeperBehaviour::new();
        assert!(!behaviour.is_fully_initialized());
        assert!(!behaviour.has_nat_support());
    }

    #[test]
    fn test_behaviour_full_init() {
        let mut behaviour = VaultKeeperBehaviour::new();
        behaviour.mark_kademlia_ready();
        behaviour.mark_gossip_ready();
        behaviour.mark_heartbeat_active();
        assert!(behaviour.is_fully_initialized());
    }

    #[test]
    fn test_behaviour_with_nat() {
        let behaviour = VaultKeeperBehaviour::new_with_nat();
        assert!(behaviour.has_nat_support());
        assert!(behaviour.nat_traversal_enabled);
        assert!(behaviour.relay_enabled);
        assert!(behaviour.dcutr_enabled);
        // Core protocols should NOT be initialized yet
        assert!(!behaviour.is_fully_initialized());
    }

    #[test]
    fn test_behaviour_with_nat_full_init() {
        let mut behaviour = VaultKeeperBehaviour::new_with_nat();
        behaviour.mark_kademlia_ready();
        behaviour.mark_gossip_ready();
        behaviour.mark_heartbeat_active();
        behaviour.mark_mdns_enabled();
        assert!(behaviour.is_fully_initialized());
        assert!(behaviour.has_nat_support());
        assert!(behaviour.mdns_enabled);
    }

    #[test]
    fn test_build_behaviour_identity() {
        let keypair = build_behaviour_identity();
        let peer_id = keypair.public().to_peer_id();
        assert!(!peer_id.to_string().is_empty());
    }

    #[test]
    fn test_build_behaviour_transport() {
        let keypair = build_behaviour_identity();
        let result = build_behaviour_transport(&keypair);
        assert!(result.is_ok());
    }
}
