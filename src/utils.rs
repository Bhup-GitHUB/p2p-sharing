use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use mime_guess;

pub fn get_local_ip() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    
    match addr.ip() {
        IpAddr::V4(ip) => Some(ip),
        IpAddr::V6(_) => None,
    }
}

pub fn get_broadcast_address() -> String {
    if let Some(ip) = get_local_ip() {
        let octets = ip.octets();
        format!("{}.{}.{}.255", octets[0], octets[1], octets[2])
    } else {
        "255.255.255.255".to_string()
    }
}

pub async fn calculate_file_checksum(file_path: &Path) -> anyhow::Result<String> {
    let mut file = File::open(file_path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 65536]; // 64KB buffer
    
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    
    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

pub fn get_mime_type(file_path: &Path) -> Option<String> {
    mime_guess::from_path(file_path)
        .first()
        .map(|m| m.to_string())
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

pub fn format_speed(bytes_per_sec: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec))
}

pub fn calculate_eta(remaining_bytes: u64, speed_bytes_per_sec: u64) -> Option<u64> {
    if speed_bytes_per_sec == 0 {
        return None;
    }
    Some(remaining_bytes / speed_bytes_per_sec)
}

