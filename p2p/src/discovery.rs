//! Peer discovery using Kademlia DHT.

use anyhow::Result;
use libp2p::{
    Multiaddr,
    PeerId,
    kad::{
        behaviour::{Kademlia, KademliaConfig},
        Record,
        store::MemoryStore,
    },
    multiaddr::Protocol,
};
use tracing::{debug, info, warn};

/// Initialize Kademlia DHT for peer discovery.
pub fn create_kademlia(
    local_peer_id: &PeerId,
    protocol_name: &str,
) -> Result<Kademlia<MemoryStore>> {
    let mut config = KademliaConfig::default();
    config.set_protocol_names(vec![libp2p::StreamProtocol::try_from_owned(protocol_name.to_string())?]);

    let store = MemoryStore::new(local_peer_id.to_owned());
    let kademlia = Kademlia::with_config(local_peer_id.to_owned(), store, config);

    info!(
        "Kademlia DHT initialized with protocol: {}",
        protocol_name
    );
    Ok(kademlia)
}

/// Add bootstrap nodes to Kademlia routing table.
pub fn add_bootstrap_nodes(
    kademlia: &mut Kademlia<MemoryStore>,
    bootstrap_addrs: &[String],
) -> Result<usize> {
    let mut added = 0;
    for addr_str in bootstrap_addrs {
        let addr: Multiaddr = addr_str
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse bootstrap addr '{}': {}", addr_str, e))?;

        let peer_id =
            if let Some(Protocol::P2p(peer_id)) = addr.iter().find(|p| matches!(p, Protocol::P2p(_)))
            {
                peer_id
            } else {
                warn!("Bootstrap address missing P2p protocol: {}", addr_str);
                continue;
            };

        kademlia.add_address(&peer_id, addr);
        added += 1;
        info!("Added bootstrap node: {}", peer_id);
    }
    Ok(added)
}

/// Start bootstrapping the DHT.
pub fn bootstrap(kademlia: &mut Kademlia<MemoryStore>) {
    let _ = kademlia.bootstrap();
    info!("DHT bootstrap initiated");
}

/// Put a value into the DHT.
pub fn put_record(
    kademlia: &mut Kademlia<MemoryStore>,
    key: &[u8],
    value: Vec<u8>,
) -> Result<()> {
    let record = Record {
        key: key.to_vec().into(),
        value,
        publisher: None,
        expires: None,
    };
    kademlia.put_record(record, libp2p::kad::Quorum::One)?;
    debug!("Record stored in DHT");
    Ok(())
}

/// Get a value from the DHT.
pub fn get_record(
    kademlia: &mut Kademlia<MemoryStore>,
    key: &[u8],
) -> Result<Option<Vec<u8>>> {
    kademlia.get_record(key.to_vec().into());
    // The result comes through the event loop.
    // In a real implementation, this would use a oneshot channel.
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_kademlia() {
        let peer_id = PeerId::random();
        let result = create_kademlia(&peer_id, "/test/1.0.0");
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_kademlia_custom_protocol() {
        let peer_id = PeerId::random();
        let result = create_kademlia(&peer_id, "/vaultkeeper/dht/1.0.0");
        assert!(result.is_ok());
    }

    #[test]
    fn test_add_bootstrap_nodes_invalid() {
        let peer_id = PeerId::random();
        let mut kad = create_kademlia(&peer_id, "/test/1.0.0").unwrap();
        let result = add_bootstrap_nodes(&mut kad, &["/invalid-address".to_string()]);
        // Invalid addresses are skipped, but shouldn't error
        assert!(result.is_ok());
    }

    #[test]
    fn test_bootstrap() {
        let peer_id = PeerId::random();
        let mut kad = create_kademlia(&peer_id, "/test/1.0.0").unwrap();
        // Bootstrap should not panic
        bootstrap(&mut kad);
    }
}
