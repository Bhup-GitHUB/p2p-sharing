use crate::peer::Peer;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    GetPeers,
    SendFile {
        peer_id: Uuid,
        file_path: String,
    },
    BroadcastFile {
        file_path: String,
    },
    GetLocalInfo,
    SendChat {
        peer_id: Option<Uuid>,
        message: String,
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
        file_size: u64,
    },
    FileTransferProgress {
        transfer_id: Uuid,
        progress: u64,
        total: u64,
    },
    FileTransferComplete {
        transfer_id: Uuid,
        peer_id: Option<Uuid>,
    },
    FileTransferError {
        transfer_id: Uuid,
        peer_id: Option<Uuid>,
        message: String,
    },
    BroadcastTransferStart {
        transfer_id: Uuid,
        filename: String,
        file_size: u64,
        total_peers: usize,
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
    Pong,
    Error {
        message: String,
    },
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

