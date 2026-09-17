use crate::models::state::AppState;
use axum::{extract::State, http::StatusCode};

/// Completes a manual scan before acknowledging it so reloading sees the new index.
pub async fn refresh_index_handler(State(state): State<AppState>) -> StatusCode {
    match state.refresh_index().await {
        Ok(()) => StatusCode::OK,
        Err(error) => {
            log::error!("Manual index refresh failed: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
