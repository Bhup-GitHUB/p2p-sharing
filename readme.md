# 🚀 AirDrop Killer - P2P File Bomber

> Drop files to anyone on your LAN network. No server. No bullshit. Just pure peer-to-peer magic.

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey.svg)

## 🔥 Features

- **🔍 Auto-Discovery** - Automatically finds all devices running the app on your LAN
- **📁 Drag & Drop** - Dead simple file sharing interface
- **⚡ Multi-Transfer** - Send files to multiple people simultaneously
- **💬 Integrated Chat** - Talk while you share
- **📡 Broadcast Mode** - Send one file to everyone at once
- **🎯 True P2P** - No central server needed. Direct device-to-device transfer
- **📊 Real-time Progress** - Watch your transfers in real-time
- **🔒 LAN Only** - Works only on your local network (safe by default)

## 🎯 Use Cases

- Share assignment files with your entire study group instantly
- Transfer large project files between lab computers
- Quick file exchange in hackathons
- Share memes with the whole hostel floor
- Collaborative work without USB drives or cloud uploads

## 📋 Prerequisites

- **Rust** 1.75 or higher ([Install Rust](https://rustup.rs/))
- All devices must be on the **same LAN/WiFi network**
- Firewall must allow the app (or disable temporarily for testing)

## 🚀 Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/Bhup-GITHUB/airdrop-killer.git
cd airdrop-killer
# Build the project
cargo build --release

# Run the application
cargo run --release
```

The app will:
1. Start a web server on `http://localhost:3030`
2. Begin broadcasting its presence on the network
3. Listen for other devices

### First Time Setup

1. **Run on multiple devices** - Install and run on at least 2 computers on the same network
2. **Open your browser** - Go to `http://localhost:3030` on each device
3. **Wait a moment** - Devices will auto-discover each other (takes 2-5 seconds)
4. **Start sharing** - Drag and drop files to send!

## 💻 Usage

### Sending Files

1. Open the web interface
2. See all connected devices in the sidebar
3. Select a device or use "Broadcast to All"
4. Drag & drop files or click to browse
5. Watch the magic happen!

### Chat

- Click the chat icon next to any device
- Send messages while transferring files
- Broadcast messages to everyone

### Broadcast Mode

- Click "Broadcast Mode" button
- Select your file
- It will be sent to ALL connected devices simultaneously

## 🏗️ Architecture

```
┌─────────────────────────────────────┐
│      Web UI (Browser)               │
│   React + WebSocket Client          │
└───────────────┬─────────────────────┘
                │ WebSocket
┌───────────────▼─────────────────────┐
│         Rust Backend                │
│  ┌─────────────────────────────┐   │
│  │  Web Server (Axum)          │   │
│  │  - Serves UI                │   │
│  │  - WebSocket handler        │   │
│  └─────────────────────────────┘   │
│  ┌─────────────────────────────┐   │
│  │  Discovery Service          │   │
│  │  - UDP Broadcast (port 7878)│   │
│  │  - Peer management          │   │
│  └─────────────────────────────┘   │
│  ┌─────────────────────────────┐   │
│  │  File Transfer Engine       │   │
│  │  - TCP Listener (port 7879) │   │
│  │  - Chunked transfers        │   │
│  │  - Progress tracking        │   │
│  └─────────────────────────────┘   │
└─────────────────────────────────────┘
```

## 🔧 Configuration

Edit `config.toml` (auto-generated on first run):

```toml
[network]
discovery_port = 7878      # UDP broadcast port
transfer_port = 7879       # TCP file transfer port
web_port = 3030           # Web UI port
broadcast_interval = 2     # Discovery broadcast interval (seconds)

[transfer]
chunk_size = 65536        # File chunk size (64KB)
max_concurrent = 5        # Max simultaneous transfers

[ui]
theme = "dark"            # "dark" or "light"
```

## 📁 Project Structure

```
p2p-sharing/
├── src/
│   ├── main.rs              # Entry point, app initialization
│   ├── discovery.rs         # UDP broadcast & peer discovery
│   ├── transfer.rs          # TCP file transfer logic
│   ├── peer.rs              # Peer connection management
│   ├── websocket.rs         # WebSocket handler for UI
│   ├── chat.rs              # Chat message handling
│   └── web/                 # Static web UI files
│       ├── index.html       # Main UI
│       ├── app.js           # Frontend logic
│       └── style.css        # Styling
├── Cargo.toml               # Rust dependencies
├── config.toml              # Configuration file
└── README.md                # You are here!
```

## 🛠️ Tech Stack

### Backend (Rust)
- **axum** - Web framework
- **tokio** - Async runtime
- **tokio-tungstenite** - WebSocket implementation
- **mdns-sd** / **socket2** - Network discovery
- **serde** - JSON serialization

## 🐛 Troubleshooting

### Devices not discovering each other?

- Make sure all devices are on the **same subnet**
- Check firewall settings (allow UDP 7878 and TCP 7879)
- Some enterprise WiFi networks block peer-to-peer traffic
- Try running: `sudo ./airdrop-killer` (Linux/Mac) if permission issues

### File transfer failing?

- Check available disk space on receiver
- Ensure TCP port 7879 is not blocked
- Try smaller files first to test connection
- Check terminal for error messages

### Web UI not loading?

- Verify nothing else is using port 3030
- Try `http://127.0.0.1:3030` instead of `localhost`
- Check browser console for errors

## 🚧 Development

### Build from source

```bash
# Debug build (faster compilation)
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

### Contributing

Pull requests are welcome! For major changes:

1. Fork the repo
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## 📝 Roadmap

- [ ] Encryption for file transfers (TLS)
- [ ] File preview before accepting
- [ ] Transfer history
- [ ] Dark/Light theme toggle
- [ ] Mobile app (iOS/Android)
- [ ] Resume interrupted transfers
- [ ] Folder sharing
- [ ] Speed limit controls
- [ ] Custom device names/avatars

## ⚠️ Security Notes

- This app is designed for **trusted LAN environments** (like your college network with friends)
- Files are transferred **unencrypted** (currently)
- Anyone on the network running the app can see your device
- Don't use on public/untrusted networks without encryption
- Future versions will include TLS encryption
