use crate::models::state::AppState;
use crate::utils::scanner::{ScanDirectoryParams, scan_directory};
use axum::{extract::State, http::StatusCode};
use log::{info, warn};
use std::cmp::Reverse;
use std::time::SystemTime;

/// Handles a manual index-refresh request from the client UI (`POST /refresh`).
///
/// Offloads the CPU/IO-heavy scan to a blocking thread pool, then atomically
/// replaces the in-memory video cache with the freshly sorted results.
pub async fn refresh_index_handler(State(state): State<AppState>) -> StatusCode {
    let dir = state.movie_directory.clone();
    let depth = state.depth;
    let cache_clone = state.index_cache.clone();

    // Offload blocking filesystem I/O to the blocking thread pool.
    tokio::task::spawn_blocking(move || {
        info!("Manual index refresh triggered. Scanning...");
        let start_time = SystemTime::now();

        let mut params = ScanDirectoryParams::new(dir, depth);
        scan_directory(&mut params);

        let count = params.count;
        let mut videos = params.videos;
        videos.sort_by_key(|v: &crate::models::video::VideoFile| Reverse(v.unix_timestamp));

        let duration = start_time.elapsed().map(|d| d.as_millis()).unwrap_or(0);

        if let Ok(mut writer) = cache_clone.try_write() {
            let found = videos.len();
            *writer = videos;
            print!("\r\x1b[2K");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            println!(
                "\x1b[1;32m[SUCCESS]\x1b[0m Manual index refresh complete in {duration}ms. \
                 Found {found} video(s) out of {count} scanned item(s)."
            );
        } else {
            warn!("Failed to acquire write lock for manual rescan.");
        }
    });

    StatusCode::OK
}
