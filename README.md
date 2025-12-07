# Axi-Vid

A simple 1:1 video chat application using Axum (Rust) and WebRTC.

## Overview

Axi-Vid enables peer-to-peer video/audio calls between two users in a shared room. The Axum server handles WebSocket-based signaling (SDP offers/answers, ICE candidates) while actual media streams flow directly between peers via WebRTC.

## Features

- 1:1 video and audio calls
- WebRTC peer-to-peer connections (no media server)
- Text chat alongside video
- Mute/unmute audio and video
- Room-based connections with shareable links
- Mobile-friendly responsive UI
- Automatic reconnection on connection drops

## Requirements

- Rust 1.70+
- Modern browser with WebRTC support (Chrome, Firefox, Safari, Edge)

## Quick Start

```bash
# Clone and enter directory
cd axi-vid

# Run the server
cargo run

# Open in browser
# http://localhost:3000
```

The app will automatically create a new room and redirect you to it. Share the URL with another person to start a video call.

## Usage

1. Open http://localhost:3000 in your browser
2. Click "Copy Link" to share the room URL
3. Open the URL in another browser/tab
4. Click "Start Call" on both ends
5. Allow camera/microphone access when prompted


## Signaling Messages

Messages are JSON with a `type` field:

```json
{"type": "offer", "sdp": "..."}
{"type": "answer", "sdp": "..."}
{"type": "ice", "candidate": "...", "sdpMLineIndex": 0, "sdpMid": "0"}
{"type": "chat", "message": "Hello"}
{"type": "media_status", "audio": true, "video": false}
```

For external testing (different networks):

```bash
# Use ngrok for NAT traversal
ngrok http 3000
```

## Troubleshooting

### Camera/Microphone not working
- Check browser permissions (click lock icon in address bar)
- Ensure no other app is using the camera
- Try a different browser

### Calls not connecting
- Both peers must be in the same room
- Check firewall settings
- For external connections, use ngrok or similar
- Symmetric NAT may require a TURN server

### Adding TURN server

If direct connections fail, add a TURN server to `app.js`:

```javascript
const CONFIG = {
    iceServers: [
        { urls: 'stun:stun.l.google.com:19302' },
        {
            urls: 'turn:your-turn-server.com:3478',
            username: 'user',
            credential: 'pass'
        }
    ],
    // ...
};
```

## Extensions

### Group calls
For more than 2 participants, consider using an SFU (Selective Forwarding Unit) like:
- mediasoup
- Janus
- pion/ion

### Authentication
Add JWT authentication:

```rust
// Add to Cargo.toml
jsonwebtoken = "9"

// Verify token in ws_handler before upgrade
```

### HTTPS

For production, use a reverse proxy (nginx) with TLS or add rustls:

```rust
// Add to Cargo.toml
axum-server = { version = "0.6", features = ["tls-rustls"] }
```

## License

MIT
