//! Composite network behaviour combining all P2P protocols.

use anyhow::Result;
use libp2p::{
    Transport,
    core::upgrade::Version,
    identity::Keypair as IdentityKeypair,
    noise, tcp, yamux, PeerId,
};
use tracing::info;

/// Composite behaviour for the VaultKeeper P2P node.
/// In a full implementation, this would use #[derive(NetworkBehaviour)]
/// to combine Kademlia, GossipSub, Ping, and custom protocols.
///
/// For this implementation, we provide the setup infrastructure.
pub struct VaultKeeperBehaviour {
    /// Kademlia DHT for peer discovery
    pub kademlia_initialized: bool,
    /// GossipSub for message broadcasting
    pub gossip_initialized: bool,
    /// Heartbeat monitoring active
    pub heartbeat_active: bool,
}

impl VaultKeeperBehaviour {
    pub fn new() -> Self {
        Self {
            kademlia_initialized: false,
            gossip_initialized: false,
            heartbeat_active: false,
        }
    }

    pub fn mark_kademlia_ready(&mut self) {
        self.kademlia_initialized = true;
    }

    pub fn mark_gossip_ready(&mut self) {
        self.gossip_initialized = true;
    }

    pub fn mark_heartbeat_active(&mut self) {
        self.heartbeat_active = true;
    }

    pub fn is_fully_initialized(&self) -> bool {
        self.kademlia_initialized && self.gossip_initialized && self.heartbeat_active
    }
}

impl Default for VaultKeeperBehaviour {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a libp2p transport with noise encryption and yamux multiplexing.
pub fn build_transport(
    keypair: &IdentityKeypair,
) -> Result<
    impl Transport<Output = (PeerId, libp2p::core::muxing::StreamMuxerBox)>
        + Send
        + Unpin
        + 'static,
> {
    let noise_config = noise::Config::new(keypair)?;
    let transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
        .upgrade(Version::V1Lazy)
        .authenticate(noise_config)
        .multiplex(yamux::Config::default())
        .boxed();

    info!("P2P transport configured: TCP + Noise + Yamux");
    Ok(transport)
}

/// Generate a new Ed25519 identity keypair for the node.
pub fn generate_identity() -> IdentityKeypair {
    let keypair = IdentityKeypair::generate_ed25519();
    info!(
        "Generated new Ed25519 node identity: {}",
        keypair.public().to_peer_id()
    );
    keypair
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_behaviour_default() {
        let behaviour = VaultKeeperBehaviour::new();
        assert!(!behaviour.is_fully_initialized());
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
    fn test_generate_identity() {
        let keypair = generate_identity();
        let peer_id = keypair.public().to_peer_id();
        assert!(!peer_id.to_string().is_empty());
    }

    #[test]
    fn test_build_transport() {
        let keypair = generate_identity();
        let result = build_transport(&keypair);
        assert!(result.is_ok());
    }
}
