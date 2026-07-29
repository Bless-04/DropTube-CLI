use crate::models::state::AppState;
use crate::utils::scanner::scan_directory;
use axum::{extract::State, http::StatusCode};
use log::{info, warn};
use std::time::SystemTime;

/// Handles manual scan trigger endpoint from client UI
pub async fn refresh_index_handler(State(state): State<AppState>) -> StatusCode {
    let dir = state.movie_directory.clone();
    let recurse = state.recurse;
    let cache_clone = state.index_cache.clone();

    // CPU/IO-heavy scan on blocking thread pool
    tokio::task::spawn_blocking(move || {
        //println!("\n\x1b[1;33m[INFO]\x1b[0m Manual index refresh triggered. Scanning...");
        info!("Manual index refresh triggered. Scanning...");
        let mut list = Vec::new();
        let mut count = 0;
        let start_time = SystemTime::now();
        scan_directory(&dir, &dir, recurse, &mut list, &mut count);
        list.sort_by_key(|v| std::cmp::Reverse(v.unix_timestamp));

        let duration = start_time.elapsed().map(|d| d.as_millis()).unwrap_or(0);

        // Update index cache
        if let Ok(mut writer) = cache_clone.try_write() {
            *writer = list;
            print!("\r\x1b[2K");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            println!(
                "\x1b[1;32m[SUCCESS]\x1b[0m Manual index refresh complete in {}ms. Found {} video(s) out of {} scanned item(s).",
                duration,
                writer.len(),
                count
            );
        } else {
            warn!("Failed to acquire write lock for manual rescan.");
        }
    });

    StatusCode::OK
}
