pub mod handlers;

use axum::Router;
use axum::routing::{get, post};
use log::info;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::services::ServeDir;

use crate::models::state::AppState;
use handlers::{home_page_handler, explorer_root_handler, explorer_path_handler, refresh_index_handler};

pub fn create_router(state: AppState) -> Router {
    let router = Router::new()
        .route("/", get(home_page_handler))
        .nest_service("/public", ServeDir::new("public"))
        .route("/refresh", post(refresh_index_handler))
        .route("/explorer", get(explorer_root_handler))
        .route("/explorer/{*path}", get(explorer_path_handler))
        .nest_service("/video", ServeDir::new(&state.movie_directory))
        .layer(CatchPanicLayer::new()) // Catch requests panic to keep daemon running
        .with_state(state);
    info!("All Routes Loaded");
    router
}
