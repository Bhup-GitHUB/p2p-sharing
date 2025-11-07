mod config;
mod discovery;
mod peer;
mod transfer;
mod utils;
mod websocket;

use anyhow::Result;
use config::AppConfig;
use discovery::DiscoveryService;
use transfer::TransferService;
use websocket::WebSocketService;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config = AppConfig::load()?;
    let config = Arc::new(config);

    let peers = Arc::new(RwLock::new(peer::PeerManager::new()));

    let transfer_service = TransferService::new(config.clone());
    
    let transfer_task = tokio::spawn(async move {
        if let Err(e) = transfer_service.start_listener().await {
            tracing::error!("Transfer listener error: {}", e);
        }
    });

    let mut discovery = DiscoveryService::new(
        config.clone(),
        peers.clone(),
    ).await?;

    let discovery_task = tokio::spawn(async move {
        if let Err(e) = discovery.start().await {
            tracing::error!("Discovery service error: {}", e);
        }
    });

    let websocket_service = Arc::new(WebSocketService::new(
        config.clone(),
        peers.clone(),
    ));

    let websocket_task = tokio::spawn(async move {
        if let Err(e) = websocket_service.start_server().await {
            tracing::error!("WebSocket server error: {}", e);
        }
    });

    tokio::select! {
        _ = transfer_task => {},
        _ = discovery_task => {},
        _ = websocket_task => {},
    }

    Ok(())
}

