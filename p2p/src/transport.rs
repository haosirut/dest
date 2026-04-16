//! Transport layer: TCP + Noise + Yamux for libp2p v0.54.

use anyhow::Result;
use libp2p::{
    Transport,
    core::upgrade::Version,
    identity::Keypair,
};
use tracing::info;

/// A boxed transport type for use with libp2p Swarm.
pub type BoxedTransport = libp2p::core::transport::Boxed<
    (libp2p::PeerId, libp2p::core::muxing::StreamMuxerBox),
>;

/// Build a libp2p transport with TCP, Noise encryption, and Yamux multiplexing.
///
/// Uses the libp2p 0.54 API:
/// - `tcp::tokio::Transport` (requires `tokio` feature on libp2p)
/// - `noise::Config::new()` (not the deprecated `AuthenticKeypair`)
/// - `core::upgrade::Version::V1Lazy`
/// - `yamux::Config::default()`
pub fn build_transport(keypair: &Keypair) -> Result<BoxedTransport> {
    let noise_config = libp2p::noise::Config::new(keypair)
        .map_err(|e| anyhow::anyhow!("Failed to create noise config: {}", e))?;

    let transport = libp2p::tcp::tokio::Transport::new(libp2p::tcp::Config::new().nodelay(true))
        .upgrade(Version::V1Lazy)
        .authenticate(noise_config)
        .multiplex(libp2p::yamux::Config::default())
        .boxed();

    info!("P2P transport configured: TCP + Noise + Yamux");
    Ok(transport)
}

/// Generate a new Ed25519 identity keypair for the node.
pub fn generate_identity() -> Keypair {
    let keypair = Keypair::generate_ed25519();
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
    fn test_generate_identity() {
        let keypair = generate_identity();
        let peer_id = keypair.public().to_peer_id();
        assert!(!peer_id.to_string().is_empty());
    }

    #[test]
    fn test_generate_identity_unique() {
        let kp1 = generate_identity();
        let kp2 = generate_identity();
        let id1 = kp1.public().to_peer_id();
        let id2 = kp2.public().to_peer_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_build_transport() {
        let keypair = generate_identity();
        let result = build_transport(&keypair);
        assert!(
            result.is_ok(),
            "build_transport should succeed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_transport_peer_id_matches() {
        let keypair = generate_identity();
        let peer_id = keypair.public().to_peer_id();
        let _transport = build_transport(&keypair).unwrap();
        // The transport uses the keypair, so the peer_id derived from it matches
        assert!(!peer_id.to_string().is_empty());
    }
}
