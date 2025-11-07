mod config;
mod discovery;
mod peer;
mod utils;

use anyhow::Result;
use config::AppConfig;
use discovery::DiscoveryService;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config = AppConfig::load()?;
    let config = Arc::new(config);

    let peers = Arc::new(RwLock::new(peer::PeerManager::new()));

    let mut discovery = DiscoveryService::new(
        config.clone(),
        peers.clone(),
    ).await?;

    discovery.start().await?;

    Ok(())
}

