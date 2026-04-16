//! Peer discovery using Kademlia DHT (libp2p kad v0.54).

use anyhow::Result;
use libp2p::{
    Multiaddr,
    PeerId,
    kad::{
        Behaviour,
        Config as KadConfig,
        Record,
        RecordKey,
        store::MemoryStore,
    },
    multiaddr::Protocol,
    StreamProtocol,
};
use tracing::{debug, info, warn};

/// Initialize Kademlia DHT for peer discovery.
///
/// Creates a Kademlia instance with the given local peer ID and protocol name.
/// Uses `MemoryStore` as the record store backend.
pub fn create_kademlia(
    local_peer_id: &PeerId,
    protocol_name: &str,
) -> Result<Behaviour<MemoryStore>> {
    let protocol =
        StreamProtocol::try_from_owned(protocol_name.to_string()).map_err(|e| {
            anyhow::anyhow!("Invalid protocol name '{}': {}", protocol_name, e)
        })?;

    let config = KadConfig::new(protocol);
    let store = MemoryStore::new(local_peer_id.to_owned());
    let kademlia = Behaviour::with_config(local_peer_id.to_owned(), store, config);

    info!(
        "Kademlia DHT initialized with protocol: {}",
        protocol_name
    );
    Ok(kademlia)
}

/// Add bootstrap nodes to the Kademlia routing table.
///
/// Parses each multiaddress string, extracts the PeerId from the `/p2p/` component,
/// and adds the address to the routing table.
///
/// Returns the number of successfully added bootstrap nodes.
pub fn add_bootstrap_nodes(
    kademlia: &mut Behaviour<MemoryStore>,
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
        debug!("Added bootstrap node: {}", peer_id);
    }
    info!("Added {} bootstrap nodes", added);
    Ok(added)
}

/// Start bootstrapping the DHT.
///
/// Initiates the Kademlia bootstrap process to populate the routing table.
pub fn bootstrap(kademlia: &mut Behaviour<MemoryStore>) {
    match kademlia.bootstrap() {
        Ok(_) => info!("DHT bootstrap initiated"),
        Err(e) => warn!("DHT bootstrap failed (no bootstrap nodes?): {:?}", e),
    }
}

/// Put a value into the DHT.
///
/// Stores a key-value record in the DHT with quorum=1 (at least one peer confirms).
pub fn put_record(
    kademlia: &mut Behaviour<MemoryStore>,
    key: &[u8],
    value: Vec<u8>,
) -> Result<()> {
    let record = Record {
        key: RecordKey::from(key.to_vec()),
        value,
        publisher: None,
        expires: None,
    };
    kademlia.put_record(record, libp2p::kad::Quorum::One)?;
    debug!("Record stored in DHT (key: {} bytes)", key.len());
    Ok(())
}

/// Get a value from the DHT by key.
///
/// Initiates a DHT lookup. The actual result arrives asynchronously via
/// the Swarm event loop. Returns `Ok(None)` immediately since the lookup
/// is non-blocking. In production, use a oneshot channel to receive the result.
pub fn get_record(
    kademlia: &mut Behaviour<MemoryStore>,
    key: &[u8],
) -> Result<Option<Vec<u8>>> {
    kademlia.get_record(RecordKey::from(key.to_vec()));
    debug!("DHT lookup initiated (key: {} bytes)", key.len());
    // The result arrives through the event loop.
    // In production, this would return via a oneshot channel.
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
        // Invalid address: missing /p2p/ component, should be skipped
        let result = add_bootstrap_nodes(&mut kad, &["/ip4/127.0.0.1/tcp/9444".to_string()]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_bootstrap_without_nodes() {
        let peer_id = PeerId::random();
        let mut kad = create_kademlia(&peer_id, "/test/1.0.0").unwrap();
        // Bootstrap should not panic even without nodes
        bootstrap(&mut kad);
    }

    #[test]
    fn test_put_record() {
        let peer_id = PeerId::random();
        let mut kad = create_kademlia(&peer_id, "/test/1.0.0").unwrap();
        let result = put_record(&mut kad, b"test_key", b"test_value".to_vec());
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_record_non_blocking() {
        let peer_id = PeerId::random();
        let mut kad = create_kademlia(&peer_id, "/test/1.0.0").unwrap();
        let result = get_record(&mut kad, b"nonexistent_key");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // Non-blocking, returns None immediately
    }
}
