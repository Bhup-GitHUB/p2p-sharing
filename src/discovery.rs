use crate::config::AppConfig;
use crate::peer::{Peer, PeerManager};
use crate::protocol::PeerInfo;
use crate::utils;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryMessage {
    pub peer_id: uuid::Uuid,
    pub address: SocketAddr,
    pub hostname: String,
}

pub struct DiscoveryService {
    config: Arc<AppConfig>,
    peers: Arc<RwLock<PeerManager>>,
    socket: UdpSocket,
    websocket_service: Option<Arc<crate::websocket::WebSocketService>>,
}

impl DiscoveryService {
    pub async fn new(config: Arc<AppConfig>, peers: Arc<RwLock<PeerManager>>) -> Result<Self> {
        let bind_addr = format!("0.0.0.0:{}", config.network.discovery_port);
        let socket = UdpSocket::bind(&bind_addr).await?;
        socket.set_broadcast(true)?;

        Ok(Self {
            config,
            peers,
            socket,
            websocket_service: None,
        })
    }

    pub fn set_websocket_service(&mut self, service: Arc<crate::websocket::WebSocketService>) {
        self.websocket_service = Some(service);
    }

    pub async fn start(&mut self) -> Result<()> {
        let socket = Arc::new(self.socket.try_clone()?);
        let config = self.config.clone();
        let peers = self.peers.clone();
        let websocket = self.websocket_service.clone();

        let broadcast_task = {
            let socket = socket.clone();
            let config = config.clone();
            let peers = peers.clone();
            tokio::spawn(async move {
                Self::broadcast_loop(socket, config, peers).await;
            })
        };

        let listen_task = {
            let socket = socket.clone();
            let peers = peers.clone();
            let websocket = websocket.clone();
            tokio::spawn(async move {
                Self::listen_loop(socket, peers, websocket).await;
            })
        };

        let cleanup_task = {
            let peers = peers.clone();
            let websocket = websocket.clone();
            tokio::spawn(async move {
                Self::cleanup_loop(peers, websocket).await;
            })
        };

        tokio::select! {
            _ = broadcast_task => {},
            _ = listen_task => {},
            _ = cleanup_task => {},
        }

        Ok(())
    }

    async fn broadcast_loop(
        socket: Arc<UdpSocket>,
        config: Arc<AppConfig>,
        peers: Arc<RwLock<PeerManager>>,
    ) {
        let mut interval = interval(Duration::from_secs(config.network.broadcast_interval));
        let broadcast_addr = format!("{}:{}", utils::get_broadcast_address(), config.network.discovery_port);

        let local_ip = utils::get_local_ip().unwrap_or(Ipv4Addr::new(127, 0, 0, 1));
        let transfer_addr = SocketAddr::new(IpAddr::V4(local_ip), config.network.transfer_port);

        loop {
            interval.tick().await;

            let peer_manager = peers.read().await;
            let message = DiscoveryMessage {
                peer_id: peer_manager.local_id(),
                address: transfer_addr,
                hostname: peer_manager.local_hostname().to_string(),
            };

            if let Ok(data) = serde_json::to_vec(&message) {
                let _ = socket.send_to(&data, &broadcast_addr).await;
            }
        }
    }

    async fn listen_loop(
        socket: Arc<UdpSocket>,
        peers: Arc<RwLock<PeerManager>>,
        websocket: Option<Arc<crate::websocket::WebSocketService>>,
    ) {
        let mut buf = [0u8; 1024];

        loop {
            match socket.recv_from(&mut buf).await {
                Ok((size, addr)) => {
                    if let Ok(message) = serde_json::from_slice::<DiscoveryMessage>(&buf[..size]) {
                        let mut peer_manager = peers.write().await;
                        if message.peer_id != peer_manager.local_id() {
                            let was_new = !peer_manager.get_peer(&message.peer_id).is_some();
                            let peer = Peer::from_discovery(message.peer_id, message.address, message.hostname.clone());
                            peer_manager.add_or_update_peer(peer);
                            tracing::info!("Discovered peer: {} from {}", message.hostname, addr);
                            
                            if was_new {
                                if let Some(ws) = &websocket {
                                    let peer_info = PeerInfo {
                                        id: message.peer_id,
                                        address: message.address,
                                        hostname: message.hostname,
                                    };
                                    ws.notify_peer_discovered(peer_info).await;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error receiving discovery message: {}", e);
                }
            }
        }
    }

    async fn cleanup_loop(
        peers: Arc<RwLock<PeerManager>>,
        websocket: Option<Arc<crate::websocket::WebSocketService>>,
    ) {
        let mut interval = interval(Duration::from_secs(10));

        loop {
            interval.tick().await;
            let mut peer_manager = peers.write().await;
            let before_peers: std::collections::HashSet<_> = peer_manager.list_peers()
                .iter()
                .map(|p| p.id)
                .collect();
            
            peer_manager.cleanup_stale_peers(30);
            
            let after_peers: std::collections::HashSet<_> = peer_manager.list_peers()
                .iter()
                .map(|p| p.id)
                .collect();

            if let Some(ws) = &websocket {
                for removed_id in before_peers.difference(&after_peers) {
                    ws.notify_peer_removed(*removed_id).await;
                }
            }
        }
    }
}

