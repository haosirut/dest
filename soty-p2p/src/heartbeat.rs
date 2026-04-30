//! Heartbeat protocol: nodes exchange pings every 15 minutes to maintain liveness.

use crate::message::{P2pMessage};
use chrono::Utc;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::debug;

/// Peer health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerStatus {
    /// Peer is alive and responding
    Alive,
    /// Peer missed last heartbeat
    Suspect,
    /// Peer declared dead after multiple missed heartbeats
    Dead,
}

/// Peer state tracked by heartbeat
#[derive(Debug, Clone)]
pub struct PeerState {
    pub peer_id: String,
    pub status: PeerStatus,
    pub last_seen: Instant,
    pub available_space_bytes: u64,
    pub missed_heartbeats: u32,
    pub consecutive_failures: u32,
}

impl PeerState {
    pub fn new(peer_id: String) -> Self {
        Self {
            peer_id,
            status: PeerStatus::Alive,
            last_seen: Instant::now(),
            available_space_bytes: 0,
            missed_heartbeats: 0,
            consecutive_failures: 0,
        }
    }
}

/// Configuration for heartbeat
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    pub interval_secs: u64,
    pub max_missed: u32,
    pub suspect_threshold: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_secs: 900, // 15 minutes
            max_missed: 3,
            suspect_threshold: 1,
        }
    }
}

/// Heartbeat manager
pub struct HeartbeatManager {
    pub config: HeartbeatConfig,
    peers: HashMap<String, PeerState>,
    node_id: String,
    pub available_space: u64,
}

impl HeartbeatManager {
    pub fn new(node_id: String, config: HeartbeatConfig) -> Self {
        Self {
            config,
            peers: HashMap::new(),
            node_id,
            available_space: 0,
        }
    }

    /// Create a heartbeat message
    pub fn create_heartbeat(&self) -> P2pMessage {
        P2pMessage::Heartbeat {
            node_id: self.node_id.clone(),
            timestamp: Utc::now().timestamp() as u64,
            available_space_bytes: self.available_space,
        }
    }

    /// Process an incoming heartbeat message
    pub fn process_heartbeat(
        &mut self,
        node_id: &str,
        _timestamp: u64,
        available_space: u64,
    ) -> PeerEvent {
        let now = Instant::now();
        let peer = self
            .peers
            .entry(node_id.to_string())
            .or_insert_with(|| PeerState::new(node_id.to_string()));

        peer.last_seen = now;
        peer.available_space_bytes = available_space;
        peer.consecutive_failures = 0;

        let event = match peer.status {
            PeerStatus::Dead | PeerStatus::Suspect => {
                peer.status = PeerStatus::Alive;
                peer.missed_heartbeats = 0;
                PeerEvent::PeerRecovered(node_id.to_string())
            }
            PeerStatus::Alive => PeerEvent::PeerAlive(node_id.to_string()),
        };

        debug!(
            "Heartbeat from {} (space: {} bytes)",
            node_id, available_space
        );
        event
    }

    /// Mark a peer as having missed a heartbeat
    pub fn tick(&mut self) -> Vec<PeerEvent> {
        let now = Instant::now();
        let interval = Duration::from_secs(self.config.interval_secs);
        let mut events = Vec::new();

        for peer in self.peers.values_mut() {
            let elapsed = now.duration_since(peer.last_seen);
            if elapsed > interval {
                peer.missed_heartbeats += 1;

                match peer.status {
                    PeerStatus::Alive => {
                        if peer.missed_heartbeats >= self.config.suspect_threshold {
                            peer.status = PeerStatus::Suspect;
                            events.push(PeerEvent::PeerSuspect(peer.peer_id.clone()));
                        }
                    }
                    PeerStatus::Suspect => {
                        if peer.missed_heartbeats >= self.config.max_missed {
                            peer.status = PeerStatus::Dead;
                            events.push(PeerEvent::PeerDied(peer.peer_id.clone()));
                        }
                    }
                    PeerStatus::Dead => {
                        peer.consecutive_failures += 1;
                    }
                }
            }
        }

        events
    }

    /// Get all dead peers that need data replication
    pub fn get_dead_peers(&self) -> Vec<&PeerState> {
        self.peers
            .values()
            .filter(|p| p.status == PeerStatus::Dead)
            .collect()
    }

    /// Get alive peers count
    pub fn alive_count(&self) -> usize {
        self.peers.values().filter(|p| p.status == PeerStatus::Alive).count()
    }

    /// Update available space for this node
    pub fn set_available_space(&mut self, space: u64) {
        self.available_space = space;
    }

    /// Register a peer
    pub fn register_peer(&mut self, peer_id: String) {
        self.peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerState::new(peer_id));
    }
}

/// Events emitted by the heartbeat manager
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerEvent {
    PeerAlive(String),
    PeerSuspect(String),
    PeerDied(String),
    PeerRecovered(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_heartbeat_message() {
        let manager = HeartbeatManager::new("node1".to_string(), HeartbeatConfig::default());
        let msg = manager.create_heartbeat();
        match msg {
            P2pMessage::Heartbeat { node_id, .. } => assert_eq!(node_id, "node1"),
            _ => panic!("Expected Heartbeat message"),
        }
    }

    #[test]
    fn test_process_heartbeat_new_peer() {
        let mut manager =
            HeartbeatManager::new("node1".to_string(), HeartbeatConfig::default());
        let event = manager.process_heartbeat("node2", 1000, 1024);
        assert_eq!(event, PeerEvent::PeerAlive("node2".to_string()));
        assert_eq!(manager.alive_count(), 1);
    }

    #[test]
    fn test_peer_status_transitions() {
        let mut manager = HeartbeatManager::new(
            "node1".to_string(),
            HeartbeatConfig {
                interval_secs: 1,
                suspect_threshold: 1,
                max_missed: 2,
            },
        );

        manager.process_heartbeat("node2", 1000, 1024);
        assert_eq!(
            manager.peers.get("node2").unwrap().status,
            PeerStatus::Alive
        );

        // Simulate missed heartbeats by manipulating last_seen
        {
            let peer = manager.peers.get_mut("node2").unwrap();
            peer.last_seen = Instant::now() - Duration::from_secs(5);
        }
        let events = manager.tick();
        assert!(events
            .iter()
            .any(|e| *e == PeerEvent::PeerSuspect("node2".to_string())));

        {
            let peer = manager.peers.get_mut("node2").unwrap();
            peer.last_seen = Instant::now() - Duration::from_secs(15);
        }
        let events = manager.tick();
        assert!(events
            .iter()
            .any(|e| *e == PeerEvent::PeerDied("node2".to_string())));
    }

    #[test]
    fn test_peer_recovery() {
        let mut manager =
            HeartbeatManager::new("node1".to_string(), HeartbeatConfig::default());
        manager.process_heartbeat("node2", 1000, 1024);

        {
            let peer = manager.peers.get_mut("node2").unwrap();
            peer.status = PeerStatus::Dead;
        }

        let event = manager.process_heartbeat("node2", 2000, 2048);
        assert_eq!(event, PeerEvent::PeerRecovered("node2".to_string()));
        assert_eq!(manager.peers.get("node2").unwrap().status, PeerStatus::Alive);
    }

    #[test]
    fn test_register_peer() {
        let mut manager =
            HeartbeatManager::new("node1".to_string(), HeartbeatConfig::default());
        manager.register_peer("node2".to_string());
        assert!(manager.peers.contains_key("node2"));
    }

    #[test]
    fn test_set_available_space() {
        let mut manager =
            HeartbeatManager::new("node1".to_string(), HeartbeatConfig::default());
        manager.set_available_space(1024 * 1024 * 1024);
        assert_eq!(manager.available_space, 1024 * 1024 * 1024);
    }
}
