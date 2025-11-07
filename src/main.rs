mod config;
mod discovery;
mod peer;

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

    let discovery = DiscoveryService::new(
        config.clone(),
        peers.clone(),
    )?;

    discovery.start().await?;

    Ok(())
}

