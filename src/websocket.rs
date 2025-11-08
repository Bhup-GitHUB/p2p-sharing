use crate::config::AppConfig;
use crate::history::TransferHistory;
use crate::peer::PeerManager;
use crate::protocol::{ClientMessage, ServerMessage, PeerInfo};
use crate::transfer::TransferService;
use crate::utils;
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
    client_to_peer: Arc<RwLock<HashMap<Uuid, Uuid>>>,
    transfer_service: Arc<TransferService>,
    history: Arc<TransferHistory>,
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
            client_to_peer: Arc::new(RwLock::new(HashMap::new())),
            transfer_service,
            history: Arc::new(TransferHistory::new(1000)), // Keep last 1000 transfers
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

    pub async fn add_connection(&self, client_id: Uuid, peer_id: Uuid, tx: mpsc::UnboundedSender<Message>) {
        let mut connections = self.connections.write().await;
        connections.insert(client_id, tx);
        let mut client_to_peer = self.client_to_peer.write().await;
        client_to_peer.insert(client_id, peer_id);
        tracing::info!("WebSocket client connected: {} (peer: {})", client_id, peer_id);
    }

    pub async fn remove_connection(&self, client_id: &Uuid) {
        let mut connections = self.connections.write().await;
        connections.remove(client_id);
        let mut client_to_peer = self.client_to_peer.write().await;
        client_to_peer.remove(client_id);
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
        self: Arc<Self>,
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
                    let file_path = PathBuf::from(&file_path);
                    if file_path.exists() && file_path.is_file() {
                        let filename = file_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let file_size = std::fs::metadata(&file_path)?.len();
                        
                        // Create history record
                        let transfer_id = Uuid::new_v4();
                        let history_record = crate::history::TransferRecord::new(
                            transfer_id,
                            Some(peer_id),
                            peer.hostname.clone(),
                            filename.clone(),
                            file_path.to_string_lossy().to_string(),
                            file_size,
                            "sent".to_string(),
                        );
                        self.history.start_transfer(history_record).await;
                        
                        let transfer_service = self.transfer_service.clone();
                        let history = self.history.clone();
                        let websocket_service = self.clone();
                        let client_id_clone = client_id;
                        
                        tokio::spawn(async move {
                            match transfer_service.send_file(peer.address, file_path).await {
                                Ok(_) => {
                                    // Note: checksum verification would be done in transfer service
                                    history.complete_transfer(&transfer_id, None, true).await;
                                }
                                Err(e) => {
                                    history.fail_transfer(&transfer_id).await;
                                    let error_msg = ServerMessage::FileTransferError {
                                        transfer_id,
                                        peer_id: Some(peer_id),
                                        message: e.to_string(),
                                    };
                                    let json = serde_json::to_string(&error_msg).unwrap_or_default();
                                    let _ = websocket_service.send_to_client(
                                        &client_id_clone,
                                        axum::extract::ws::Message::Text(json),
                                    ).await;
                                }
                            }
                        });
                        
                        Ok(Some(ServerMessage::FileTransferRequest {
                            transfer_id,
                            peer_id,
                            filename,
                            file_path: file_path.to_string_lossy().to_string(),
                            file_size,
                            file_checksum: None, // Will be calculated during transfer
                            mime_type: utils::get_mime_type(&file_path),
                        }))
                    } else {
                        Ok(Some(ServerMessage::Error {
                            message: "File not found or is not a file".to_string(),
                        }))
                    }
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Peer not found".to_string(),
                    }))
                }
            }
            ClientMessage::BroadcastFile { file_path } => {
                let peers = self.peers.read().await;
                let peer_list = peers.list_peers();
                let file_path = PathBuf::from(file_path);
                
                if !file_path.exists() {
                    return Ok(Some(ServerMessage::Error {
                        message: "File not found".to_string(),
                    }));
                }

                let file_metadata = std::fs::metadata(&file_path)?;
                let filename = file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let file_size = file_metadata.len();

                let broadcast_id = Uuid::new_v4();
                let total_peers = peer_list.len();

                if total_peers == 0 {
                    return Ok(Some(ServerMessage::Error {
                        message: "No peers available for broadcast".to_string(),
                    }));
                }

                let file_checksum = utils::calculate_file_checksum(&file_path).await.ok();
                let mime_type = utils::get_mime_type(&file_path);
                
                let start_msg = ServerMessage::BroadcastTransferStart {
                    transfer_id: broadcast_id,
                    filename: filename.clone(),
                    file_path: file_path.to_string_lossy().to_string(),
                    file_size,
                    total_peers,
                    file_checksum,
                    mime_type,
                };
                let json = serde_json::to_string(&start_msg).unwrap_or_default();
                let ws_msg = axum::extract::ws::Message::Text(json);
                self.send_to_client(&client_id, ws_msg).await?;

                let transfer_service = self.transfer_service.clone();
                let websocket_service = self.clone();
                let client_id_clone = client_id;

                tokio::spawn(async move {
                    let mut successful = 0;
                    let mut failed = 0;
                    let mut completed = 0;

                    for peer in peer_list {
                        let result = transfer_service
                            .send_file(peer.address, file_path.clone())
                            .await;

                        completed += 1;

                        match result {
                            Ok(_) => {
                                successful += 1;
                            }
                            Err(e) => {
                                failed += 1;
                                let error_msg = ServerMessage::FileTransferError {
                                    transfer_id: broadcast_id,
                                    peer_id: Some(peer.id),
                                    message: e.to_string(),
                                };
                                let json = serde_json::to_string(&error_msg).unwrap_or_default();
                                let _ = websocket_service.send_to_client(
                                    &client_id_clone,
                                    axum::extract::ws::Message::Text(json),
                                ).await;
                            }
                        }

                        let progress_msg = ServerMessage::BroadcastTransferProgress {
                            transfer_id: broadcast_id,
                            completed_peers: completed,
                            total_peers,
                        };
                        let json = serde_json::to_string(&progress_msg).unwrap_or_default();
                        let _ = websocket_service.send_to_client(
                            &client_id_clone,
                            axum::extract::ws::Message::Text(json),
                        ).await;
                    }

                    let complete_msg = ServerMessage::BroadcastTransferComplete {
                        transfer_id: broadcast_id,
                        successful_peers: successful,
                        failed_peers: failed,
                    };
                    let json = serde_json::to_string(&complete_msg).unwrap_or_default();
                    let _ = websocket_service.send_to_client(
                        &client_id_clone,
                        axum::extract::ws::Message::Text(json),
                    ).await;
                });

                Ok(None)
            }
            ClientMessage::SendChat { peer_id, message } => {
                let peers = self.peers.read().await;
                let client_to_peer = self.client_to_peer.read().await;
                let from_peer_id = client_to_peer.get(&client_id)
                    .copied()
                    .unwrap_or_else(|| peers.local_id());
                let from_hostname = peers.local_hostname().to_string();

                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let chat_msg = ServerMessage::ChatMessage {
                    from_peer_id,
                    from_hostname,
                    to_peer_id: peer_id,
                    message,
                    timestamp,
                };

                let json = serde_json::to_string(&chat_msg).unwrap_or_default();
                let ws_msg = axum::extract::ws::Message::Text(json.clone());

                if let Some(target_peer_id) = peer_id {
                    let connections = self.connections.read().await;
                    let client_to_peer = self.client_to_peer.read().await;
                    
                    for (cid, peer_id_map) in client_to_peer.iter() {
                        if *peer_id_map == target_peer_id || *cid == client_id {
                            if let Some(tx) = connections.get(cid) {
                                let _ = tx.send(ws_msg.clone());
                            }
                        }
                    }
                } else {
                    self.broadcast_to_all(ws_msg).await;
                }

                Ok(None)
            }
            ClientMessage::GetTransferHistory => {
                let history_entries = self.history.get_all_history().await;
                Ok(Some(ServerMessage::TransferHistory {
                    transfers: history_entries,
                }))
            }
            ClientMessage::GetTransferStats { transfer_id } => {
                if let Some(record) = self.history.get_transfer(&transfer_id).await {
                    Ok(Some(ServerMessage::TransferStats {
                        transfer_id,
                        status: record.status,
                        progress: 0, // Would need to track this separately
                        total: record.file_size,
                        speed_bytes_per_sec: record.speed_bytes_per_sec,
                        eta_seconds: None, // Would need to calculate
                        start_time: record.start_time,
                    }))
                } else {
                    Ok(Some(ServerMessage::Error {
                        message: "Transfer not found".to_string(),
                    }))
                }
            }
            ClientMessage::CancelTransfer { transfer_id } => {
                self.history.cancel_transfer(&transfer_id).await;
                Ok(Some(ServerMessage::TransferCancelled { transfer_id }))
            }
            ClientMessage::PauseTransfer { transfer_id } => {
                self.history.pause_transfer(&transfer_id).await;
                Ok(Some(ServerMessage::TransferPaused { transfer_id }))
            }
            ClientMessage::ResumeTransfer { transfer_id } => {
                self.history.resume_transfer(&transfer_id).await;
                Ok(Some(ServerMessage::TransferResumed { transfer_id }))
            }
            ClientMessage::SendDirectory { peer_id, dir_path } => {
                // Directory transfer would require archiving - for now return error
                Ok(Some(ServerMessage::Error {
                    message: "Directory transfer not yet implemented. Please archive the directory first.".to_string(),
                }))
            }
            ClientMessage::BroadcastDirectory { dir_path: _ } => {
                Ok(Some(ServerMessage::Error {
                    message: "Directory broadcast not yet implemented. Please archive the directory first.".to_string(),
                }))
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

    let peer_id = {
        let peers = service.peers.read().await;
        peers.local_id()
    };

    service.add_connection(client_id, peer_id, tx.clone()).await;

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
                        match service_recv.clone().handle_client_message(client_id_recv, client_msg).await {
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

