use crate::config::AppConfig;
use crate::peer::PeerManager;
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

pub struct WebSocketService {
    config: Arc<AppConfig>,
    peers: Arc<RwLock<PeerManager>>,
    connections: Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
}

impl WebSocketService {
    pub fn new(config: Arc<AppConfig>, peers: Arc<RwLock<PeerManager>>) -> Self {
        Self {
            config,
            peers,
            connections: Arc::new(RwLock::new(HashMap::new())),
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

    service.add_connection(client_id, tx).await;

    let (mut sender, mut receiver) = socket.split();

    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let service_clone = service.clone();
    let client_id_clone = client_id;

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    tracing::debug!("Received text message from {}: {}", client_id_clone, text);
                }
                Message::Binary(data) => {
                    tracing::debug!("Received binary message from {}: {} bytes", client_id_clone, data.len());
                }
                Message::Close(_) => {
                    break;
                }
                Message::Ping(data) => {
                    if let Err(e) = sender.send(Message::Pong(data)).await {
                        tracing::error!("Failed to send pong: {}", e);
                        break;
                    }
                }
                Message::Pong(_) => {}
            }
        }
        service_clone.remove_connection(&client_id_clone).await;
    });

    tokio::select! {
        _ = send_task => {
            recv_task.abort();
        }
        _ = recv_task => {
            send_task.abort();
        }
    }
}

