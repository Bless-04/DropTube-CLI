use crate::models::video::VideoFile;
use crate::utils::scanner::{ScanDirectoryParams, scan_directory};
use crate::utils::thumbnails::ThumbnailGenerator;
use std::path::PathBuf;
use std::sync::Arc;
use std::{cmp::Reverse, io};
use tokio::sync::Mutex;
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
    /// Optional, startup-validated FFmpeg generator. None means no generation or cache writes.
    pub thumbnails: Option<ThumbnailGenerator>,
    /// Serializes full scans so background and manual refreshes cannot race.
    pub scan_lock: Arc<Mutex<()>>,
}

impl AppState {
    /// Scans off the async runtime, generates missing thumbnails, then publishes the index.
    /// Existing sidecar images always take priority over generated thumbnails.
    pub async fn refresh_index(&self) -> io::Result<()> {
        let _scan_guard = self.scan_lock.lock().await;
        let directory = self.movie_directory.clone();
        let depth = self.depth;
        let mut videos = tokio::task::spawn_blocking(move || {
            let mut params = ScanDirectoryParams::new(directory, depth);
            scan_directory(&mut params);
            params.videos
        })
        .await
        .map_err(io::Error::other)?;
        videos.sort_by_key(|video| Reverse(video.unix_timestamp));
        if let Some(generator) = &self.thumbnails {
            for video in &mut videos {
                if video.thumbnail_path.is_some() {
                    continue;
                }
                match generator
                    .generate_thumbnail(&self.movie_directory.join(&video.file_name))
                    .await
                {
                    Ok(path) => match path.strip_prefix(&self.movie_directory) {
                        Ok(relative) => {
                            video.thumbnail_path =
                                Some(relative.to_string_lossy().replace('\\', "/"))
                        }
                        Err(error) => log::warn!("Thumbnail path is outside the library: {error}"),
                    },
                    Err(error) => log::warn!(
                        "Thumbnail generation failed for '{}': {error}",
                        video.file_name
                    ),
                }
            }
        }
        log::info!("Index refresh complete: {} video(s)", videos.len());
        *self.index_cache.write().await = videos;
        Ok(())
    }
}

/// Query parameters accepted by the home page handler.
#[derive(serde::Deserialize)]
pub struct HomeQuery {
    /// Optional video filename to auto-play on load.
    pub v: Option<String>,
    /// Current page number (1-indexed). Defaults to 1.
    pub page: Option<u32>,
    /// Search query to filter videos by display name.
    pub search: Option<String>,
    /// Tag filter to show only videos with this tag.
    pub tag: Option<String>,
}
