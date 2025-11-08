use crate::peer::Peer;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;
use chrono;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    GetPeers,
    SendFile {
        peer_id: Uuid,
        file_path: String,
    },
    SendDirectory {
        peer_id: Uuid,
        dir_path: String,
    },
    BroadcastFile {
        file_path: String,
    },
    BroadcastDirectory {
        dir_path: String,
    },
    GetLocalInfo,
    SendChat {
        peer_id: Option<Uuid>,
        message: String,
    },
    GetTransferHistory,
    GetTransferStats {
        transfer_id: Uuid,
    },
    CancelTransfer {
        transfer_id: Uuid,
    },
    PauseTransfer {
        transfer_id: Uuid,
    },
    ResumeTransfer {
        transfer_id: Uuid,
    },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    PeersList {
        peers: Vec<PeerInfo>,
    },
    LocalInfo {
        peer_id: Uuid,
        hostname: String,
    },
    PeerDiscovered {
        peer: PeerInfo,
    },
    PeerRemoved {
        peer_id: Uuid,
    },
    FileTransferRequest {
        transfer_id: Uuid,
        peer_id: Uuid,
        filename: String,
        file_path: String,
        file_size: u64,
        file_checksum: Option<String>,
        mime_type: Option<String>,
    },
    FileTransferProgress {
        transfer_id: Uuid,
        progress: u64,
        total: u64,
        speed_bytes_per_sec: Option<u64>,
        eta_seconds: Option<u64>,
    },
    FileTransferComplete {
        transfer_id: Uuid,
        peer_id: Option<Uuid>,
        file_checksum: Option<String>,
        verified: bool,
    },
    FileTransferError {
        transfer_id: Uuid,
        peer_id: Option<Uuid>,
        message: String,
    },
    BroadcastTransferStart {
        transfer_id: Uuid,
        filename: String,
        file_path: String,
        file_size: u64,
        total_peers: usize,
        file_checksum: Option<String>,
        mime_type: Option<String>,
    },
    BroadcastTransferProgress {
        transfer_id: Uuid,
        completed_peers: usize,
        total_peers: usize,
    },
    BroadcastTransferComplete {
        transfer_id: Uuid,
        successful_peers: usize,
        failed_peers: usize,
    },
    ChatMessage {
        from_peer_id: Uuid,
        from_hostname: String,
        to_peer_id: Option<Uuid>,
        message: String,
        timestamp: u64,
    },
    TransferHistory {
        transfers: Vec<TransferHistoryEntry>,
    },
    TransferStats {
        transfer_id: Uuid,
        status: String,
        progress: u64,
        total: u64,
        speed_bytes_per_sec: Option<u64>,
        eta_seconds: Option<u64>,
        start_time: Option<chrono::DateTime<chrono::Utc>>,
    },
    TransferCancelled {
        transfer_id: Uuid,
    },
    TransferPaused {
        transfer_id: Uuid,
    },
    TransferResumed {
        transfer_id: Uuid,
    },
    Pong,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferHistoryEntry {
    pub transfer_id: Uuid,
    pub peer_id: Option<Uuid>,
    pub peer_hostname: String,
    pub filename: String,
    pub file_size: u64,
    pub direction: String, // "sent" or "received"
    pub status: String, // "completed", "failed", "cancelled"
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub duration_seconds: Option<u64>,
    pub speed_bytes_per_sec: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: Uuid,
    pub address: SocketAddr,
    pub hostname: String,
}

impl From<Peer> for PeerInfo {
    fn from(peer: Peer) -> Self {
        Self {
            id: peer.id,
            address: peer.address,
            hostname: peer.hostname,
        }
    }
}

