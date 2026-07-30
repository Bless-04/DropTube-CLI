use crate::models::video::VideoFile;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared application state passed to every Axum handler via [`axum::extract::State`].
#[derive(Clone)]
pub struct AppState {
    /// The canonical root directory being served.
    pub movie_directory: PathBuf,
    /// The port the server is listening on.
    pub port: u16,
    /// Maximum subfolder recursion depth (`0` = top-level only, `255` = unlimited).
    pub depth: u8,
    /// The in-memory video index, refreshed in the background every 30 seconds.
    pub index_cache: Arc<RwLock<Vec<VideoFile>>>,
}

/// Query parameters accepted by the home page handler.
#[derive(serde::Deserialize)]
pub struct HomeQuery {
    /// Optional video filename to auto-play on load.
    pub v: Option<String>,
}
