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
    StatusCode::OK
}

/// Handles homepage requests. Lists video files in the served directory.
/// Renders a dynamic player if the query param `v` is set.
pub async fn home_page_handler(
    State(state): State<AppState>,
    Query(query): Query<HomeQuery>,
) -> Html<String> {
    let port = state.port;
    Html(full_html)
}
