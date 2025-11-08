use crate::protocol::TransferHistoryEntry;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRecord {
    pub transfer_id: Uuid,
    pub peer_id: Option<Uuid>,
    pub peer_hostname: String,
    pub filename: String,
    pub file_path: String,
    pub file_size: u64,
    pub direction: String, // "sent" or "received"
    pub status: String, // "in_progress", "completed", "failed", "cancelled", "paused"
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_seconds: Option<u64>,
    pub speed_bytes_per_sec: Option<u64>,
    pub file_checksum: Option<String>,
    pub verified: bool,
}

impl TransferRecord {
    pub fn new(
        transfer_id: Uuid,
        peer_id: Option<Uuid>,
        peer_hostname: String,
        filename: String,
        file_path: String,
        file_size: u64,
        direction: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            transfer_id,
            peer_id,
            peer_hostname,
            filename,
            file_path,
            file_size,
            direction,
            status: "in_progress".to_string(),
            timestamp: now,
            start_time: Some(now),
            end_time: None,
            duration_seconds: None,
            speed_bytes_per_sec: None,
            file_checksum: None,
            verified: false,
        }
    }

    pub fn complete(&mut self, checksum: Option<String>, verified: bool) {
        self.status = "completed".to_string();
        self.end_time = Some(Utc::now());
        self.file_checksum = checksum;
        self.verified = verified;
        
        if let (Some(start), Some(end)) = (self.start_time, self.end_time) {
            let duration = end.signed_duration_since(start);
            self.duration_seconds = Some(duration.num_seconds() as u64);
            
            if self.duration_seconds.unwrap_or(0) > 0 {
                self.speed_bytes_per_sec = Some(self.file_size / self.duration_seconds.unwrap());
            }
        }
    }

    pub fn fail(&mut self) {
        self.status = "failed".to_string();
        self.end_time = Some(Utc::now());
        
        if let (Some(start), Some(end)) = (self.start_time, self.end_time) {
            let duration = end.signed_duration_since(start);
            self.duration_seconds = Some(duration.num_seconds() as u64);
        }
    }

    pub fn cancel(&mut self) {
        self.status = "cancelled".to_string();
        self.end_time = Some(Utc::now());
        
        if let (Some(start), Some(end)) = (self.start_time, self.end_time) {
            let duration = end.signed_duration_since(start);
            self.duration_seconds = Some(duration.num_seconds() as u64);
        }
    }

    pub fn pause(&mut self) {
        self.status = "paused".to_string();
    }

    pub fn resume(&mut self) {
        self.status = "in_progress".to_string();
    }

    pub fn update_speed(&mut self, speed: u64) {
        self.speed_bytes_per_sec = Some(speed);
    }

    pub fn to_history_entry(&self) -> TransferHistoryEntry {
        TransferHistoryEntry {
            transfer_id: self.transfer_id,
            peer_id: self.peer_id,
            peer_hostname: self.peer_hostname.clone(),
            filename: self.filename.clone(),
            file_size: self.file_size,
            direction: self.direction.clone(),
            status: self.status.clone(),
            timestamp: self.timestamp,
            duration_seconds: self.duration_seconds,
            speed_bytes_per_sec: self.speed_bytes_per_sec,
        }
    }
}

pub struct TransferHistory {
    transfers: Arc<RwLock<HashMap<Uuid, TransferRecord>>>,
    completed_transfers: Arc<RwLock<Vec<TransferRecord>>>,
    max_history: usize,
}

impl TransferHistory {
    pub fn new(max_history: usize) -> Self {
        Self {
            transfers: Arc::new(RwLock::new(HashMap::new())),
            completed_transfers: Arc::new(RwLock::new(Vec::new())),
            max_history,
        }
    }

    pub async fn start_transfer(&self, record: TransferRecord) {
        let mut transfers = self.transfers.write().await;
        transfers.insert(record.transfer_id, record);
    }

    pub async fn get_transfer(&self, transfer_id: &Uuid) -> Option<TransferRecord> {
        let transfers = self.transfers.read().await;
        transfers.get(transfer_id).cloned()
    }

    pub async fn complete_transfer(&self, transfer_id: &Uuid, checksum: Option<String>, verified: bool) {
        let mut transfers = self.transfers.write().await;
        if let Some(mut record) = transfers.remove(transfer_id) {
            record.complete(checksum, verified);
            let mut completed = self.completed_transfers.write().await;
            completed.push(record);
            
            // Keep only the last max_history entries
            if completed.len() > self.max_history {
                completed.remove(0);
            }
        }
    }

    pub async fn fail_transfer(&self, transfer_id: &Uuid) {
        let mut transfers = self.transfers.write().await;
        if let Some(mut record) = transfers.remove(transfer_id) {
            record.fail();
            let mut completed = self.completed_transfers.write().await;
            completed.push(record);
            
            if completed.len() > self.max_history {
                completed.remove(0);
            }
        }
    }

    pub async fn cancel_transfer(&self, transfer_id: &Uuid) {
        let mut transfers = self.transfers.write().await;
        if let Some(mut record) = transfers.remove(transfer_id) {
            record.cancel();
            let mut completed = self.completed_transfers.write().await;
            completed.push(record);
            
            if completed.len() > self.max_history {
                completed.remove(0);
            }
        }
    }

    pub async fn pause_transfer(&self, transfer_id: &Uuid) {
        let mut transfers = self.transfers.write().await;
        if let Some(record) = transfers.get_mut(transfer_id) {
            record.pause();
        }
    }

    pub async fn resume_transfer(&self, transfer_id: &Uuid) {
        let mut transfers = self.transfers.write().await;
        if let Some(record) = transfers.get_mut(transfer_id) {
            record.resume();
        }
    }

    pub async fn update_speed(&self, transfer_id: &Uuid, speed: u64) {
        let mut transfers = self.transfers.write().await;
        if let Some(record) = transfers.get_mut(transfer_id) {
            record.update_speed(speed);
        }
    }

    pub async fn get_all_history(&self) -> Vec<TransferHistoryEntry> {
        let active = {
            let transfers = self.transfers.read().await;
            transfers.values().map(|r| r.to_history_entry()).collect::<Vec<_>>()
        };
        
        let completed = {
            let completed = self.completed_transfers.read().await;
            completed.iter().map(|r| r.to_history_entry()).collect::<Vec<_>>()
        };
        
        let mut all = active;
        all.extend(completed);
        all.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all
    }

    pub async fn get_active_transfers(&self) -> Vec<TransferRecord> {
        let transfers = self.transfers.read().await;
        transfers.values().cloned().collect()
    }
}

