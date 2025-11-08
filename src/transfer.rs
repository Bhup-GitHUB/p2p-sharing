use crate::config::AppConfig;
use crate::utils;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio::time::{timeout, Duration};
use uuid::Uuid;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransferMessage {
    Request {
        transfer_id: Uuid,
        filename: String,
        file_path: String,
        file_size: u64,
        file_checksum: Option<String>,
        mime_type: Option<String>,
    },
    Accept {
        transfer_id: Uuid,
    },
    Reject {
        transfer_id: Uuid,
        reason: Option<String>,
    },
    Chunk {
        transfer_id: Uuid,
        chunk_index: u64,
        data: Vec<u8>,
    },
    Complete {
        transfer_id: Uuid,
        file_checksum: Option<String>,
    },
    Error {
        transfer_id: Uuid,
        message: String,
    },
    Pause {
        transfer_id: Uuid,
    },
    Resume {
        transfer_id: Uuid,
    },
    Cancel {
        transfer_id: Uuid,
    },
}

pub struct TransferService {
    config: Arc<AppConfig>,
    semaphore: Arc<Semaphore>,
}

impl TransferService {
    pub fn new(config: Arc<AppConfig>) -> Self {
        let max_concurrent = config.transfer.max_concurrent;
        Self {
            config,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    pub async fn start_listener(&self) -> Result<()> {
        let bind_addr = format!("0.0.0.0:{}", self.config.network.transfer_port);
        let listener = TcpListener::bind(&bind_addr).await?;
        tracing::info!("Transfer listener started on {}", bind_addr);

        loop {
            let (stream, addr) = listener.accept().await?;
            let config = self.config.clone();
            let semaphore = self.semaphore.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_receiver(stream, config, semaphore).await {
                    tracing::error!("Transfer receiver error from {}: {}", addr, e);
                }
            });
        }
    }

    async fn read_message(reader: &mut BufReader<&mut TcpStream>) -> Result<TransferMessage> {
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let message: TransferMessage = serde_json::from_str(line.trim())?;
        Ok(message)
    }

    async fn write_message(stream: &mut TcpStream, message: &TransferMessage) -> Result<()> {
        let data = serde_json::to_string(message)?;
        stream.write_all(data.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        Ok(())
    }

    async fn handle_receiver(
        mut stream: TcpStream,
        config: Arc<AppConfig>,
        semaphore: Arc<Semaphore>,
    ) -> Result<()> {
        let _permit = semaphore.acquire().await?;
        let mut reader = BufReader::new(&mut stream);
        
        let message = timeout(Duration::from_secs(30), Self::read_message(&mut reader)).await??;

        match message {
            TransferMessage::Request {
                transfer_id,
                filename,
                file_path: _,
                file_size,
                file_checksum: expected_checksum,
                mime_type: _,
            } => {
                let downloads_dir = std::env::current_dir()?.join("downloads");
                std::fs::create_dir_all(&downloads_dir)?;
                
                let file_path = downloads_dir.join(&filename);
                let mut file = File::create(&file_path).await?;

                let accept_msg = TransferMessage::Accept { transfer_id };
                Self::write_message(&mut stream, &accept_msg).await?;

                let mut received_size = 0u64;
                let mut chunk_index = 0u64;
                let mut hasher = Sha256::new();
                let start_time = std::time::Instant::now();

                while received_size < file_size {
                    let chunk_msg = timeout(
                        Duration::from_secs(60),
                        Self::read_message(&mut reader)
                    ).await??;
                    
                    match chunk_msg {
                        TransferMessage::Chunk {
                            transfer_id: tid,
                            chunk_index: idx,
                            data,
                        } => {
                            if tid == transfer_id && idx == chunk_index {
                                file.write_all(&data).await?;
                                hasher.update(&data);
                                received_size += data.len() as u64;
                                chunk_index += 1;
                                
                                // Log progress every 10MB
                                if received_size % (10 * 1024 * 1024) == 0 {
                                    let elapsed = start_time.elapsed().as_secs_f64();
                                    let speed = if elapsed > 0.0 {
                                        (received_size as f64 / elapsed) as u64
                                    } else {
                                        0
                                    };
                                    tracing::debug!(
                                        "Receiving {}: {}/{} ({:.1}%) - {}",
                                        filename,
                                        utils::format_bytes(received_size),
                                        utils::format_bytes(file_size),
                                        (received_size as f64 / file_size as f64) * 100.0,
                                        utils::format_speed(speed)
                                    );
                                }
                            }
                        }
                        TransferMessage::Complete { 
                            transfer_id: tid,
                            file_checksum: received_checksum,
                        } => {
                            if tid == transfer_id {
                                break;
                            }
                        }
                        TransferMessage::Cancel { transfer_id: tid } => {
                            if tid == transfer_id {
                                tracing::info!("Transfer {} cancelled by sender", transfer_id);
                                return Ok(());
                            }
                        }
                        _ => {}
                    }
                }

                file.sync_all().await?;
                
                // Verify checksum if provided
                let calculated_checksum = hex::encode(hasher.finalize());
                let verified = if let Some(expected) = expected_checksum {
                    calculated_checksum == expected
                } else {
                    true // No checksum to verify
                };
                
                if !verified {
                    tracing::warn!(
                        "Checksum mismatch for {}: expected {:?}, got {}",
                        filename,
                        expected_checksum,
                        calculated_checksum
                    );
                } else {
                    tracing::info!(
                        "File received: {} ({} bytes) - Checksum verified: {}",
                        filename,
                        received_size,
                        verified
                    );
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub async fn send_file(
        &self,
        peer_address: std::net::SocketAddr,
        file_path: PathBuf,
    ) -> Result<Uuid> {
        let transfer_id = Uuid::new_v4();
        let _permit = self.semaphore.acquire().await?;

        // Calculate checksum and get metadata
        let file_checksum = utils::calculate_file_checksum(&file_path).await.ok();
        let mime_type = utils::get_mime_type(&file_path);
        
        let mut file = File::open(&file_path).await?;
        let metadata = file.metadata().await?;
        let file_size = metadata.len();
        let filename = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let file_path_str = file_path.to_string_lossy().to_string();

        // Connect with timeout
        let stream = timeout(
            Duration::from_secs(10),
            TcpStream::connect(peer_address)
        ).await??;
        let mut stream = stream;
        let mut reader = BufReader::new(&mut stream);

        let request = TransferMessage::Request {
            transfer_id,
            filename: filename.clone(),
            file_path: file_path_str.clone(),
            file_size,
            file_checksum: file_checksum.clone(),
            mime_type,
        };
        Self::write_message(&mut stream, &request).await?;

        let response = timeout(
            Duration::from_secs(30),
            Self::read_message(&mut reader)
        ).await??;

        match response {
            TransferMessage::Accept { transfer_id: tid } => {
                if tid != transfer_id {
                    return Err(anyhow::anyhow!("Transfer ID mismatch"));
                }
            }
            TransferMessage::Reject { reason } => {
                return Err(anyhow::anyhow!(
                    "Transfer rejected by peer: {}",
                    reason.unwrap_or_else(|| "No reason provided".to_string())
                ));
            }
            _ => {
                return Err(anyhow::anyhow!("Unexpected response"));
            }
        }

        let chunk_size = self.config.transfer.chunk_size;
        let mut buffer = vec![0u8; chunk_size];
        let mut chunk_index = 0u64;
        let mut sent_size = 0u64;
        let start_time = std::time::Instant::now();

        // Reset file to beginning
        file = File::open(&file_path).await?;

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            let chunk = TransferMessage::Chunk {
                transfer_id,
                chunk_index,
                data: buffer[..n].to_vec(),
            };

            Self::write_message(&mut stream, &chunk).await?;

            sent_size += n as u64;
            chunk_index += 1;
            
            // Log progress every 10MB
            if sent_size % (10 * 1024 * 1024) == 0 {
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    (sent_size as f64 / elapsed) as u64
                } else {
                    0
                };
                tracing::debug!(
                    "Sending {}: {}/{} ({:.1}%) - {}",
                    filename,
                    utils::format_bytes(sent_size),
                    utils::format_bytes(file_size),
                    (sent_size as f64 / file_size as f64) * 100.0,
                    utils::format_speed(speed)
                );
            }
        }

        let complete = TransferMessage::Complete {
            transfer_id,
            file_checksum,
        };
        Self::write_message(&mut stream, &complete).await?;

        let elapsed = start_time.elapsed().as_secs_f64();
        let speed = if elapsed > 0.0 {
            (sent_size as f64 / elapsed) as u64
        } else {
            0
        };
        
        tracing::info!(
            "File sent: {} ({} bytes) in {:.2}s - {}",
            filename,
            sent_size,
            elapsed,
            utils::format_speed(speed)
        );

        Ok(transfer_id)
    }
}

