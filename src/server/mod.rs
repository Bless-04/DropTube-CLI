mod handlers;
mod protected_paths;
mod tracking;

pub use tracking::{ClientSnapshot, ServerState, TrackingListener};

use axum::Router;
use axum::middleware;
use axum::routing::{get, post};
use log::info;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::services::ServeDir;

use crate::models::state::AppState;
use crate::server::handlers::explorer::*;
use crate::server::handlers::home::home_page_handler;
use crate::server::handlers::profiles;
use crate::server::handlers::scan::refresh_index_handler;

/// creates the router for App
pub fn create_router(state: AppState) -> Router {
    let media = Router::new()
        .fallback_service(ServeDir::new(&state.movie_directory))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            protected_paths::protect_video,
        ));
    let router = Router::new()
        .route("/", get(home_page_handler))
        .route("/public/{*path}", get(handlers::assets::static_handler))
        .route("/refresh", post(refresh_index_handler))
        .route("/explorer", get(explorer_root_handler))
        .route("/explorer/{*path}", get(explorer_path_handler))
        .route(
            "/api/user/profile",
            get(profiles::get_profile).post(profiles::select_profile),
        )
        .route("/api/user/metadata", get(profiles::get_metadata))
        .route("/api/user/rating", post(profiles::set_rating))
        .route(
            "/api/user/tag",
            post(profiles::add_tag).delete(profiles::remove_tag),
        )
        .route("/api/videos/ratings", get(profiles::get_ratings))
        .nest("/video", media)
        .layer(CatchPanicLayer::new()) // Catch requests panic to keep daemon running
        .with_state(state);
    info!("All Routes Loaded");
    router
}
