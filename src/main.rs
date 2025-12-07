//! Axi-Vid: A simple 1:1 video chat application using Axum and WebRTC
//!
//! This application provides peer-to-peer video calling through WebRTC,
//! with Axum serving as the signaling server for SDP and ICE exchange.

mod handlers;
mod models;
mod state;

use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tower_http::{
    cors::CorsLayer,
    services::ServeDir,
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::handlers::{create_room, health_check, index_redirect, room_page, room_status, ws_handler};
use crate::state::{spawn_cleanup_task, AppState};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "axi_vid=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Create shared state
    let state = AppState::new();

    // Spawn background cleanup task
    spawn_cleanup_task(state.clone());

    // Build the router
    let app = Router::new()
        // API routes
        .route("/api/create-room", post(create_room))
        .route("/api/room/{room_id}/status", get(room_status))
        .route("/health", get(health_check))
        // Room page
        .route("/", get(index_redirect))
        .route("/room/{room_id}", get(room_page))
        // WebSocket endpoint
        .route("/ws/{room_id}", get(ws_handler))
        // Static files (JS, CSS)
        .nest_service("/static", ServeDir::new("static"))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        // Shared state
        .with_state(state);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("Starting Axi-Vid server on http://{}", addr);
    info!("Open http://localhost:3000 in your browser to start a video call");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
