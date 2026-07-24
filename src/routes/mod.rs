use axum::Router;
use axum::routing::{get, post};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::services::ServeDir;
use crate::{explorer_path_handler, explorer_root_handler, home_page_handler, refresh_index_handler, AppState};

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(home_page_handler))
        .nest_service("/public", ServeDir::new("public"))
        .route("/refresh", post(refresh_index_handler))
        .route("/explorer", get(explorer_root_handler))
        .route("/explorer/*path", get(explorer_path_handler))
        .nest_service("/video", ServeDir::new(&state.movie_directory))
        .layer(CatchPanicLayer::new()) // Catch requests panic to keep daemon running
        .with_state(state)
}