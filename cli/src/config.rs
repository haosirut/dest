//! Node configuration management.
//!
//! Handles loading, saving, and generating node identity using libp2p
//! Ed25519 keypairs. Configuration is stored as JSON in `<data_dir>/config.json`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Node configuration persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// libp2p peer ID (base58-encoded).
    pub peer_id: String,
    /// Ed25519 public key bytes.
    pub public_key: Vec<u8>,
    /// Node data directory path.
    pub data_dir: String,
    /// P2P listen port.
    pub p2p_port: u16,
    /// HTTP API listen port.
    pub api_port: u16,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

impl NodeConfig {
    /// Generate a new node configuration with a fresh Ed25519 identity.
    ///
    /// Creates the identity keypair, derives the peer ID, and saves the
    /// configuration to `<data_dir>/config.json`.
    pub fn generate(data_dir: &Path) -> Result<Self> {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = keypair.public().to_peer_id();

        // Extract the raw Ed25519 public key (32 bytes) for config storage.
        let ed_keypair: libp2p::identity::ed25519::Keypair = keypair
            .try_into()
            .map_err(|_| anyhow::anyhow!("Expected Ed25519 keypair"))?;
        let public_key = ed_keypair.public().to_bytes().to_vec();

        let config = Self {
            peer_id: peer_id.to_string(),
            public_key,
            data_dir: data_dir.to_string_lossy().to_string(),
            p2p_port: 9444,
            api_port: 8080,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        config.save(data_dir)?;
        Ok(config)
    }

    /// Load configuration from `<data_dir>/config.json`.
    pub fn load(data_dir: &str) -> Result<Self> {
        let path = PathBuf::from(data_dir).join("config.json");
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Config not found at: {}", path.display()))?;
        let config: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config at: {}", path.display()))?;
        Ok(config)
    }

    /// Save configuration to `<data_dir>/config.json`.
    fn save(&self, data_dir: &Path) -> Result<()> {
        let path = data_dir.join("config.json");
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write config to: {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_and_load() {
        let tmp = TempDir::new().unwrap();
        let generated = NodeConfig::generate(tmp.path()).unwrap();

        assert!(!generated.peer_id.is_empty());
        assert!(!generated.public_key.is_empty());
        assert_eq!(generated.p2p_port, 9444);
        assert_eq!(generated.api_port, 8080);

        let loaded = NodeConfig::load(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(generated.peer_id, loaded.peer_id);
        assert_eq!(generated.public_key, loaded.public_key);
        assert_eq!(generated.created_at, loaded.created_at);
    }

    #[test]
    fn test_load_nonexistent_fails() {
        let tmp = TempDir::new().unwrap();
        let result = NodeConfig::load(tmp.path().to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = NodeConfig {
            peer_id: "12D3KooWTestPeerId".to_string(),
            public_key: vec![1, 2, 3, 4],
            data_dir: "/tmp/test".to_string(),
            p2p_port: 9444,
            api_port: 8080,
            created_at: "2024-01-01T00:00:00+00:00".to_string(),
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let deserialized: NodeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.peer_id, config.peer_id);
        assert_eq!(deserialized.public_key, config.public_key);
        assert_eq!(deserialized.p2p_port, config.p2p_port);
        assert_eq!(deserialized.api_port, config.api_port);
    }

    #[test]
    fn test_generate_creates_config_file() {
        let tmp = TempDir::new().unwrap();
        NodeConfig::generate(tmp.path()).unwrap();

        let config_path = tmp.path().join("config.json");
        assert!(config_path.exists());

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("peer_id"));
        assert!(content.contains("public_key"));
    }
}
