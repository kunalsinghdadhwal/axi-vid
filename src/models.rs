//! WebSocket message types for WebRTC signaling
//!
//! All messages are JSON-serialized and use a tagged enum pattern
//! for type discrimination.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Incoming messages from WebSocket clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    /// WebRTC SDP offer from caller
    Offer { sdp: String },

    /// WebRTC SDP answer from callee
    Answer { sdp: String },

    /// ICE candidate for NAT traversal
    #[serde(rename = "ice")]
    IceCandidate {
        candidate: String,
        #[serde(rename = "sdpMLineIndex")]
        sdp_m_line_index: u32,
        #[serde(rename = "sdpMid")]
        sdp_mid: Option<String>,
    },

    /// Peer joined notification
    Join,

    /// Peer left notification
    Leave,

    /// Text chat message
    Chat { message: String },

    /// Media status update (mute/unmute)
    MediaStatus {
        audio: bool,
        video: bool,
    },

    /// Peer status broadcast
    PeerStatus { status: String },

    /// Error message
    Error { message: String },

    /// Room info (peer count, etc.)
    RoomInfo { peer_count: usize },

    /// Ping/pong for keepalive
    Ping,
    Pong,
}

impl WsMessage {
    /// Create an error message
    pub fn error(msg: impl Into<String>) -> Self {
        WsMessage::Error {
            message: msg.into(),
        }
    }

    /// Create a room info message
    pub fn room_info(peer_count: usize) -> Self {
        WsMessage::RoomInfo { peer_count }
    }
}

/// Response for room creation
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateRoomResponse {
    /// The unique identifier for the created room (UUID)
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub room_id: String,
    /// The WebSocket URL path for connecting to this room
    #[schema(example = "/ws/550e8400-e29b-41d4-a716-446655440000")]
    pub ws_url: String,
}

/// Room status response
#[derive(Debug, Serialize, ToSchema)]
pub struct RoomStatus {
    /// The room identifier
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub room_id: String,
    /// Number of peers currently in the room
    #[schema(example = 1)]
    pub peer_count: usize,
    /// Whether the room can accept more peers (max 2)
    #[schema(example = true)]
    pub available: bool,
}
