use crate::config::AppConfig;
use crate::peer::PeerManager;
use crate::protocol::{ClientMessage, ServerMessage, PeerInfo};
use crate::transfer::TransferService;
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

pub struct WebSocketService {
    config: Arc<AppConfig>,
    peers: Arc<RwLock<PeerManager>>,
    connections: Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    transfer_service: Arc<TransferService>,
}

impl WebSocketService {
    pub fn new(
        config: Arc<AppConfig>,
        peers: Arc<RwLock<PeerManager>>,
        transfer_service: Arc<TransferService>,
    ) -> Self {
        Self {
            config,
            peers,
            connections: Arc::new(RwLock::new(HashMap::new())),
            transfer_service,
        }
    }

    pub fn create_router(self: Arc<Self>) -> Router {
        Router::new()
            .route("/ws", get(websocket_handler))
            .with_state(self)
    }

    pub async fn start_server(self: Arc<Self>) -> Result<()> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.config.network.web_port));
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!("WebSocket server started on http://{}", addr);

        let app = self.create_router();
        let server = axum::serve(listener, app);

        server.await?;
        Ok(())
    }

    pub async fn add_connection(&self, client_id: Uuid, tx: mpsc::UnboundedSender<Message>) {
        let mut connections = self.connections.write().await;
        connections.insert(client_id, tx);
        tracing::info!("WebSocket client connected: {}", client_id);
    }

    pub async fn remove_connection(&self, client_id: &Uuid) {
        let mut connections = self.connections.write().await;
        connections.remove(client_id);
        tracing::info!("WebSocket client disconnected: {}", client_id);
    }

    pub async fn broadcast_to_all(&self, message: Message) {
        let connections = self.connections.read().await;
        for (client_id, tx) in connections.iter() {
            if let Err(e) = tx.send(message.clone()) {
                tracing::warn!("Failed to send message to client {}: {}", client_id, e);
            }
        }
    }

    pub async fn send_to_client(&self, client_id: &Uuid, message: Message) -> Result<()> {
        let connections = self.connections.read().await;
        if let Some(tx) = connections.get(client_id) {
            tx.send(message)?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Client not found"))
        }
    }

    async fn handle_client_message(
        &self,
        client_id: Uuid,
        message: ClientMessage,
    ) -> Result<Option<ServerMessage>> {
        match message {
            ClientMessage::GetPeers => {
                let peers = self.peers.read().await;
                let peer_list: Vec<PeerInfo> = peers
                    .list_peers()
                    .into_iter()
                    .map(PeerInfo::from)
                    .collect();
                Ok(Some(ServerMessage::PeersList { peers: peer_list }))
            }
            ClientMessage::GetLocalInfo => {
                let peers = self.peers.read().await;
                Ok(Some(ServerMessage::LocalInfo {
                    peer_id: peers.local_id(),
                    hostname: peers.local_hostname().to_string(),
                }))
            }
            ClientMessage::SendFile { peer_id, file_path } => {
                let peers = self.peers.read().await;
                if let Some(peer) = peers.get_peer(&peer_id) {
                    let file_path = PathBuf::from(file_path);
                    if file_path.exists() {
                        let transfer_id = self
                            .transfer_service
                            .send_file(peer.address, file_path)
                            .await?;
                        Ok(Some(ServerMessage::FileTransferComplete { transfer_id }))
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "File not found".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Peer not found".to_string(),
                    }))
                }
            }
            ClientMessage::Ping => Ok(Some(ServerMessage::Pong)),
        }
    }

    pub async fn notify_peer_discovered(&self, peer: PeerInfo) {
        let message = ServerMessage::PeerDiscovered { peer };
        let json = serde_json::to_string(&message).unwrap_or_default();
        self.broadcast_to_all(Message::Text(json)).await;
    }

    pub async fn notify_peer_removed(&self, peer_id: Uuid) {
        let message = ServerMessage::PeerRemoved { peer_id };
        let json = serde_json::to_string(&message).unwrap_or_default();
        self.broadcast_to_all(Message::Text(json)).await;
    }
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(service): State<Arc<WebSocketService>>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, service))
}

async fn handle_socket(socket: WebSocket, service: Arc<WebSocketService>) {
    let client_id = Uuid::new_v4();
    let (tx, mut rx) = mpsc::unbounded_channel();

    service.add_connection(client_id, tx.clone()).await;

    let (mut sender, mut receiver) = socket.split();

    let service_send = service.clone();
    let client_id_send = client_id;

    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
        service_send.remove_connection(&client_id_send).await;
    });

    let service_recv = service.clone();
    let client_id_recv = client_id;
    let pong_tx = tx.clone();

    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                        match service_recv.handle_client_message(client_id_recv, client_msg).await {
                            Ok(Some(response)) => {
                                if let Ok(json) = serde_json::to_string(&response) {
                                    if let Err(e) = pong_tx.send(Message::Text(json)) {
                                        tracing::error!("Failed to send response: {}", e);
                                    }
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                let error_msg = ServerMessage::Error {
                                    message: e.to_string(),
                                };
                                if let Ok(json) = serde_json::to_string(&error_msg) {
                                    let _ = pong_tx.send(Message::Text(json));
                                }
                            }
                        }
                    } else {
                        tracing::warn!("Invalid message format from {}: {}", client_id_recv, text);
                    }
                }
                Message::Binary(data) => {
                    tracing::debug!("Received binary message from {}: {} bytes", client_id_recv, data.len());
                }
                Message::Close(_) => {
                    break;
                }
                Message::Ping(data) => {
                    if let Err(e) = pong_tx.send(Message::Pong(data)) {
                        tracing::error!("Failed to send pong: {}", e);
                        break;
                    }
                }
                Message::Pong(_) => {}
            }
        }
        service_recv.remove_connection(&client_id_recv).await;
    });

    tokio::select! {
        _ = send_task => {
            recv_task.abort();
        }
        _ = recv_task => {
        }
    }
}

