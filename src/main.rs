mod config;
mod discovery;
mod history;
mod peer;
mod protocol;
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

    tracing::info!("Starting P2P File Sharing Backend");
    tracing::info!("Discovery port: {}", config.network.discovery_port);
    tracing::info!("Transfer port: {}", config.network.transfer_port);
    tracing::info!("WebSocket port: {}", config.network.web_port);

    let peers = Arc::new(RwLock::new(peer::PeerManager::new()));

    let transfer_service = Arc::new(TransferService::new(config.clone()));
    let transfer_service_listener = transfer_service.clone();
    
    let transfer_task = tokio::spawn(async move {
        tracing::info!("Transfer service started");
        if let Err(e) = transfer_service_listener.start_listener().await {
            tracing::error!("Transfer listener error: {}", e);
        }
    });

    let websocket_service = Arc::new(WebSocketService::new(
        config.clone(),
        peers.clone(),
        transfer_service.clone(),
    ));

    let mut discovery = DiscoveryService::new(
        config.clone(),
        peers.clone(),
    ).await?;
    discovery.set_websocket_service(websocket_service.clone());

    let discovery_task = tokio::spawn(async move {
        tracing::info!("Discovery service started");
        if let Err(e) = discovery.start().await {
            tracing::error!("Discovery service error: {}", e);
        }
    });

    let websocket_task = tokio::spawn(async move {
        tracing::info!("WebSocket service started");
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

