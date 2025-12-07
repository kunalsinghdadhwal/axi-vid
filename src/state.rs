//! Application state management for video chat rooms
//!
//! Handles room lifecycle, peer connections, and message routing.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};

use crate::models::WsMessage;

/// Maximum peers allowed per room (1:1 video chat)
pub const MAX_PEERS_PER_ROOM: usize = 2;

/// Room inactivity timeout before cleanup
pub const ROOM_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Sender half for broadcasting messages to a peer
pub type PeerSender = mpsc::UnboundedSender<WsMessage>;

/// Represents a connected peer in a room
#[derive(Debug)]
pub struct Peer {
    pub id: String,
    pub sender: PeerSender,
    pub joined_at: Instant,
}

impl Peer {
    pub fn new(id: String, sender: PeerSender) -> Self {
        Self {
            id,
            sender,
            joined_at: Instant::now(),
        }
    }
}

/// A video chat room containing up to 2 peers
#[derive(Debug)]
pub struct Room {
    pub id: String,
    pub peers: Vec<Peer>,
    pub created_at: Instant,
    pub last_activity: Instant,
}

impl Room {
    pub fn new(id: String) -> Self {
        let now = Instant::now();
        Self {
            id,
            peers: Vec::with_capacity(MAX_PEERS_PER_ROOM),
            created_at: now,
            last_activity: now,
        }
    }

    /// Check if room is full
    pub fn is_full(&self) -> bool {
        self.peers.len() >= MAX_PEERS_PER_ROOM
    }

    /// Add a peer to the room
    pub fn add_peer(&mut self, peer: Peer) -> Result<(), &'static str> {
        if self.is_full() {
            return Err("Room is full");
        }
        self.peers.push(peer);
        self.last_activity = Instant::now();
        Ok(())
    }

    /// Remove a peer by ID
    pub fn remove_peer(&mut self, peer_id: &str) -> Option<Peer> {
        self.last_activity = Instant::now();
        if let Some(pos) = self.peers.iter().position(|p| p.id == peer_id) {
            Some(self.peers.remove(pos))
        } else {
            None
        }
    }

    /// Get the other peer in the room (for 1:1 messaging)
    pub fn get_other_peer(&self, current_peer_id: &str) -> Option<&Peer> {
        self.peers.iter().find(|p| p.id != current_peer_id)
    }

    /// Broadcast message to all peers except sender
    pub fn broadcast_to_others(&self, sender_id: &str, msg: &WsMessage) {
        for peer in &self.peers {
            if peer.id != sender_id {
                if let Err(e) = peer.sender.send(msg.clone()) {
                    warn!("Failed to send to peer {}: {}", peer.id, e);
                }
            }
        }
    }

    /// Broadcast message to all peers
    pub fn broadcast_to_all(&self, msg: &WsMessage) {
        for peer in &self.peers {
            if let Err(e) = peer.sender.send(msg.clone()) {
                warn!("Failed to send to peer {}: {}", peer.id, e);
            }
        }
    }

    /// Check if room is inactive and should be cleaned up
    pub fn is_inactive(&self) -> bool {
        self.peers.is_empty() && self.last_activity.elapsed() > ROOM_TIMEOUT
    }
}

/// Shared application state
#[derive(Debug, Clone)]
pub struct AppState {
    pub rooms: Arc<Mutex<HashMap<String, Room>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new room with given ID
    pub async fn create_room(&self, room_id: String) -> String {
        let mut rooms = self.rooms.lock().await;
        if !rooms.contains_key(&room_id) {
            info!("Creating room: {}", room_id);
            rooms.insert(room_id.clone(), Room::new(room_id.clone()));
        }
        room_id
    }

    /// Add a peer to a room, creating the room if needed
    pub async fn join_room(
        &self,
        room_id: &str,
        peer_id: String,
        sender: PeerSender,
    ) -> Result<usize, &'static str> {
        let mut rooms = self.rooms.lock().await;

        // Create room if it doesn't exist
        let room = rooms
            .entry(room_id.to_string())
            .or_insert_with(|| Room::new(room_id.to_string()));

        if room.is_full() {
            return Err("Room is full (max 2 peers for 1:1 call)");
        }

        let peer = Peer::new(peer_id.clone(), sender);
        room.add_peer(peer)?;

        let peer_count = room.peers.len();
        info!(
            "Peer {} joined room {} ({} peers)",
            peer_id, room_id, peer_count
        );

        Ok(peer_count)
    }

    /// Remove a peer from a room
    pub async fn leave_room(&self, room_id: &str, peer_id: &str) {
        let mut rooms = self.rooms.lock().await;

        if let Some(room) = rooms.get_mut(room_id) {
            if room.remove_peer(peer_id).is_some() {
                info!("Peer {} left room {}", peer_id, room_id);

                // Notify remaining peer
                room.broadcast_to_all(&WsMessage::Leave);
                room.broadcast_to_all(&WsMessage::room_info(room.peers.len()));
            }

            // Clean up empty rooms after timeout
            if room.peers.is_empty() {
                debug!("Room {} is now empty, will be cleaned up after timeout", room_id);
            }
        }
    }

    /// Forward a message to the other peer in a room
    pub async fn relay_message(&self, room_id: &str, sender_id: &str, msg: WsMessage) {
        let rooms = self.rooms.lock().await;

        if let Some(room) = rooms.get(room_id) {
            room.broadcast_to_others(sender_id, &msg);
        }
    }

    /// Get peer count for a room
    pub async fn get_peer_count(&self, room_id: &str) -> usize {
        let rooms = self.rooms.lock().await;
        rooms.get(room_id).map(|r| r.peers.len()).unwrap_or(0)
    }

    /// Clean up inactive rooms
    pub async fn cleanup_inactive_rooms(&self) {
        let mut rooms = self.rooms.lock().await;
        let before = rooms.len();

        rooms.retain(|id, room| {
            if room.is_inactive() {
                info!("Cleaning up inactive room: {}", id);
                false
            } else {
                true
            }
        });

        let removed = before - rooms.len();
        if removed > 0 {
            info!("Cleaned up {} inactive rooms", removed);
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn a background task to periodically clean up inactive rooms
pub fn spawn_cleanup_task(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            state.cleanup_inactive_rooms().await;
        }
    });
}
