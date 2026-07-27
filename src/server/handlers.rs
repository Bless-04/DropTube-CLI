pub async fn refresh_index_handler(State(state): State<AppState>) -> StatusCode {
    let dir = state.movie_directory.clone();
    let recurse = state.recurse;
    let cache_clone = state.index_cache.clone();
pub async fn home_page_handler(
    Html(full_html)
}
