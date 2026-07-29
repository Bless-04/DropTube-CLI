use crate::config::constants::HTML_SOURCE;
use crate::models::state::{AppState, HomeQuery};
use crate::models::video::{Tag, VideoFormat};
use crate::utils::scanner::scan_directory;
use crate::utils::tailwind;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use local_ip_address::local_ip;
use log::warn;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use std::fs;
use std::path::Path as StdPath;
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

/// Handles homepage requests. Lists video files in the served directory.
/// Renders a dynamic player if the query param `v` is set.
pub async fn home_page_handler(
    State(state): State<AppState>,
    Query(query): Query<HomeQuery>,
) -> Html<String> {
    let port = state.port;

    // Acquire a read lock on the cached index immediately (takes < 1ms)
    let videos = {
        let reader = state.index_cache.read().await;
        reader.clone()
    };
    Html(full_html)
}
