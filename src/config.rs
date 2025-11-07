use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub network: NetworkConfig,
    pub transfer: TransferConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub discovery_port: u16,
    pub transfer_port: u16,
    pub web_port: u16,
    pub broadcast_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferConfig {
    pub chunk_size: usize,
    pub max_concurrent: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: String,
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::config_path();
        
        if !config_path.exists() {
            let default_config = AppConfig::default();
            let toml_content = toml::to_string_pretty(&default_config)?;
            fs::write(&config_path, toml_content)?;
            return Ok(default_config);
        }

        let content = fs::read_to_string(&config_path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }

    fn config_path() -> PathBuf {
        std::env::current_dir()
            .unwrap()
            .join("config.toml")
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            network: NetworkConfig {
                discovery_port: 7878,
                transfer_port: 7879,
                web_port: 3030,
                broadcast_interval: 2,
            },
            transfer: TransferConfig {
                chunk_size: 65536,
                max_concurrent: 5,
            },
            ui: UiConfig {
                theme: "dark".to_string(),
            },
        }
    }
}

