use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub id: Uuid,
    pub address: SocketAddr,
    pub hostname: String,
    pub last_seen: std::time::SystemTime,
}

impl Peer {
    pub fn new(address: SocketAddr, hostname: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            address,
            hostname,
            last_seen: std::time::SystemTime::now(),
        }
    }

    pub fn update_seen(&mut self) {
        self.last_seen = std::time::SystemTime::now();
    }
}

pub struct PeerManager {
    peers: HashMap<Uuid, Peer>,
    local_id: Uuid,
    local_hostname: String,
}

impl PeerManager {
    pub fn new() -> Self {
        let hostname = hostname::get()
            .unwrap_or_else(|_| "unknown".into())
            .to_string_lossy()
            .to_string();

        Self {
            peers: HashMap::new(),
            local_id: Uuid::new_v4(),
            local_hostname: hostname,
        }
    }

    pub fn local_id(&self) -> Uuid {
        self.local_id
    }

    pub fn local_hostname(&self) -> &str {
        &self.local_hostname
    }

    pub fn add_or_update_peer(&mut self, peer: Peer) {
        if peer.id != self.local_id {
            self.peers.insert(peer.id, peer);
        }
    }

    pub fn remove_peer(&mut self, peer_id: &Uuid) {
        self.peers.remove(peer_id);
    }

    pub fn get_peer(&self, peer_id: &Uuid) -> Option<&Peer> {
        self.peers.get(peer_id)
    }

    pub fn list_peers(&self) -> Vec<Peer> {
        self.peers.values().cloned().collect()
    }

    pub fn cleanup_stale_peers(&mut self, timeout_secs: u64) {
        let now = std::time::SystemTime::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);

        self.peers.retain(|_, peer| {
            if let Ok(elapsed) = now.duration_since(peer.last_seen) {
                elapsed < timeout
            } else {
                false
            }
        });
    }
}

