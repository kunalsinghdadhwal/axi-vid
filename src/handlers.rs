//! HTTP and WebSocket handlers for the video chat application

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Json,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::models::{CreateRoomResponse, RoomStatus, WsMessage};
use crate::state::AppState;

/// Create a new room and return its ID
#[utoipa::path(
    post,
    path = "/api/create-room",
    tag = "Rooms",
    responses(
        (status = 200, description = "Room created successfully", body = CreateRoomResponse)
    )
)]
pub async fn create_room(State(state): State<AppState>) -> Json<CreateRoomResponse> {
    let room_id = Uuid::new_v4().to_string();
    state.create_room(room_id.clone()).await;

    Json(CreateRoomResponse {
        room_id: room_id.clone(),
        ws_url: format!("/ws/{}", room_id),
    })
}

/// Serve the room page with embedded room ID
pub async fn room_page(Path(room_id): Path<String>) -> Response {
    // Validate room ID format (should be UUID)
    if Uuid::parse_str(&room_id).is_err() {
        return (StatusCode::BAD_REQUEST, "Invalid room ID format").into_response();
    }

    // Serve the index.html with room ID injected
    let html = include_str!("/home/kunal/axi-vid/static/index.html").replace("{{ROOM_ID}}", &room_id);
    Html(html).into_response()
}

/// Redirect root to a new room
pub async fn index_redirect(State(state): State<AppState>) -> Response {
    let room_id = Uuid::new_v4().to_string();
    state.create_room(room_id.clone()).await;

    axum::response::Redirect::to(&format!("/room/{}", room_id)).into_response()
}

/// WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(room_id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    // Validate room ID
    if Uuid::parse_str(&room_id).is_err() {
        return (StatusCode::BAD_REQUEST, "Invalid room ID").into_response();
    }

    info!("WebSocket upgrade request for room: {}", room_id);

    ws.on_upgrade(move |socket| handle_socket(socket, room_id, state))
}

/// Handle an individual WebSocket connection
async fn handle_socket(socket: WebSocket, room_id: String, state: AppState) {
    let peer_id = Uuid::new_v4().to_string();
    info!("New WebSocket connection: peer {} in room {}", peer_id, room_id);

    // Create channel for sending messages to this peer
    let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();

    // Try to join the room
    let peer_count = match state.join_room(&room_id, peer_id.clone(), tx).await {
        Ok(count) => count,
        Err(e) => {
            error!("Failed to join room {}: {}", room_id, e);
            // Send error and close
            let (mut ws_tx, _) = socket.split();
            let error_msg = serde_json::to_string(&WsMessage::error(e)).unwrap();
            let _ = ws_tx.send(Message::Text(error_msg.into())).await;
            return;
        }
    };

    // Split socket into sender and receiver
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Send room info to the new peer
    let room_info = WsMessage::room_info(peer_count);
    if let Ok(msg) = serde_json::to_string(&room_info) {
        let _ = ws_tx.send(Message::Text(msg.into())).await;
    }

    // Notify other peer about the new joiner
    state
        .relay_message(&room_id, &peer_id, WsMessage::Join)
        .await;
    state
        .relay_message(&room_id, &peer_id, WsMessage::room_info(peer_count))
        .await;

    // Spawn task to forward messages from channel to WebSocket
    let ws_sender = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match serde_json::to_string(&msg) {
                Ok(text) => {
                    if ws_tx.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                }
            }
        }
    });

    // Handle incoming messages
    let room_id_clone = room_id.clone();
    let peer_id_clone = peer_id.clone();
    let state_clone = state.clone();

    let ws_receiver = async move {
        while let Some(result) = ws_rx.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    handle_text_message(&text, &room_id_clone, &peer_id_clone, &state_clone).await;
                }
                Ok(Message::Binary(data)) => {
                    // Try to parse binary as text
                    if let Ok(text) = String::from_utf8(data.to_vec()) {
                        handle_text_message(&text, &room_id_clone, &peer_id_clone, &state_clone)
                            .await;
                    }
                }
                Ok(Message::Ping(data)) => {
                    debug!("Received ping from peer {}", peer_id_clone);
                    // Pong is handled automatically by axum
                    let _ = data; // silence unused warning
                }
                Ok(Message::Pong(_)) => {
                    debug!("Received pong from peer {}", peer_id_clone);
                }
                Ok(Message::Close(_)) => {
                    info!("Peer {} closed connection", peer_id_clone);
                    break;
                }
                Err(e) => {
                    error!("WebSocket error for peer {}: {}", peer_id_clone, e);
                    break;
                }
            }
        }
    };

    // Wait for either task to complete
    tokio::select! {
        _ = ws_receiver => {
            debug!("WebSocket receiver ended for peer {}", peer_id);
        }
        _ = ws_sender => {
            debug!("WebSocket sender ended for peer {}", peer_id);
        }
    }

    // Clean up: remove peer from room
    state.leave_room(&room_id, &peer_id).await;
    info!("Peer {} disconnected from room {}", peer_id, room_id);
}

/// Process an incoming text message
async fn handle_text_message(text: &str, room_id: &str, peer_id: &str, state: &AppState) {
    // Parse the message
    let msg: WsMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            warn!("Invalid JSON from peer {}: {} - {}", peer_id, e, text);
            return;
        }
    };

    debug!("Received {:?} from peer {} in room {}", msg, peer_id, room_id);

    // Handle different message types
    match &msg {
        WsMessage::Offer { .. }
        | WsMessage::Answer { .. }
        | WsMessage::IceCandidate { .. }
        | WsMessage::Chat { .. }
        | WsMessage::MediaStatus { .. } => {
            // Relay signaling and chat messages to the other peer
            state.relay_message(room_id, peer_id, msg).await;
        }
        WsMessage::Ping => {
            // Respond with pong (application-level keepalive)
            state
                .relay_message(room_id, peer_id, WsMessage::Pong)
                .await;
        }
        WsMessage::Leave => {
            // Will be handled when connection closes
            info!("Peer {} signaling leave from room {}", peer_id, room_id);
        }
        _ => {
            debug!("Ignoring message type from peer {}", peer_id);
        }
    }
}

/// Health check endpoint
#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses(
        (status = 200, description = "Server is healthy", body = String)
    )
)]
pub async fn health_check() -> &'static str {
    "OK"
}

/// Get room status
#[utoipa::path(
    get,
    path = "/api/room/{room_id}/status",
    tag = "Rooms",
    params(
        ("room_id" = String, Path, description = "The UUID of the room")
    ),
    responses(
        (status = 200, description = "Room status retrieved successfully", body = RoomStatus)
    )
)]
pub async fn room_status(
    Path(room_id): Path<String>,
    State(state): State<AppState>,
) -> Json<RoomStatus> {
    let peer_count = state.get_peer_count(&room_id).await;

    Json(RoomStatus {
        room_id,
        peer_count,
        available: peer_count < 2,
    })
}
