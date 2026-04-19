//! Node configuration management.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub peer_id: String,
    pub public_key: Vec<u8>,
    pub data_dir: String,
    pub p2p_port: u16,
    pub api_port: u16,
    pub created_at: String,
}

impl NodeConfig {
    /// Generate a new node configuration with identity.
    pub fn generate(data_dir: &Path) -> Result<Self> {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = keypair.public().to_peer_id();

        let config = Self {
            peer_id: peer_id.to_string(),
            public_key: keypair.public().encode().to_vec(),
            data_dir: data_dir.to_string_lossy().to_string(),
            p2p_port: 9444,
            api_port: 8080,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        config.save(data_dir)?;
        Ok(config)
    }

    /// Load configuration from data directory.
    pub fn load(data_dir: &str) -> Result<Self> {
        let path = PathBuf::from(data_dir).join("config.json");
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Config not found: {}", path.display()))?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to data directory.
    fn save(&self, data_dir: &Path) -> Result<()> {
        let path = data_dir.join("config.json");
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}
