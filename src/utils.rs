use std::net::{IpAddr, Ipv4Addr};

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

