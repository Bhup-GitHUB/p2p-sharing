use crate::config::AppConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransferMessage {
    Request {
        transfer_id: Uuid,
        filename: String,
        file_size: u64,
    },
    Accept {
        transfer_id: Uuid,
    },
    Reject {
        transfer_id: Uuid,
    },
    Chunk {
        transfer_id: Uuid,
        chunk_index: u64,
        data: Vec<u8>,
    },
    Complete {
        transfer_id: Uuid,
    },
    Error {
        transfer_id: Uuid,
        message: String,
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
        
        let message = Self::read_message(&mut reader).await?;

        match message {
            TransferMessage::Request {
                transfer_id,
                filename,
                file_size,
            } => {
                let downloads_dir = std::env::current_dir()?.join("downloads");
                std::fs::create_dir_all(&downloads_dir)?;
                
                let file_path = downloads_dir.join(&filename);
                let mut file = File::create(&file_path).await?;

                let accept_msg = TransferMessage::Accept { transfer_id };
                Self::write_message(&mut stream, &accept_msg).await?;

                let mut received_size = 0u64;
                let mut chunk_index = 0u64;

                while received_size < file_size {
                    let chunk_msg = Self::read_message(&mut reader).await?;
                    
                    match chunk_msg {
                        TransferMessage::Chunk {
                            transfer_id: tid,
                            chunk_index: idx,
                            data,
                        } => {
                            if tid == transfer_id && idx == chunk_index {
                                file.write_all(&data).await?;
                                received_size += data.len() as u64;
                                chunk_index += 1;
                            }
                        }
                        TransferMessage::Complete { transfer_id: tid } => {
                            if tid == transfer_id {
                                break;
                            }
                        }
                        _ => {}
                    }
                }

                file.sync_all().await?;
                tracing::info!("File received: {} ({} bytes)", filename, received_size);
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

        let mut file = File::open(&file_path).await?;
        let metadata = file.metadata().await?;
        let file_size = metadata.len();
        let filename = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut stream = TcpStream::connect(peer_address).await?;
        let mut reader = BufReader::new(&mut stream);

        let request = TransferMessage::Request {
            transfer_id,
            filename: filename.clone(),
            file_size,
        };
        Self::write_message(&mut stream, &request).await?;

        let response = Self::read_message(&mut reader).await?;

        match response {
            TransferMessage::Accept { transfer_id: tid } => {
                if tid != transfer_id {
                    return Err(anyhow::anyhow!("Transfer ID mismatch"));
                }
            }
            TransferMessage::Reject { .. } => {
                return Err(anyhow::anyhow!("Transfer rejected by peer"));
            }
            _ => {
                return Err(anyhow::anyhow!("Unexpected response"));
            }
        }

        let chunk_size = self.config.transfer.chunk_size;
        let mut buffer = vec![0u8; chunk_size];
        let mut chunk_index = 0u64;
        let mut sent_size = 0u64;

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
        }

        let complete = TransferMessage::Complete { transfer_id };
        Self::write_message(&mut stream, &complete).await?;

        tracing::info!("File sent: {} ({} bytes)", filename, sent_size);

        Ok(transfer_id)
    }
}

