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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::sync::{Mutex, RwLock};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(prefix: &str) -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "droptube-{prefix}-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create test dir");
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn refresh_index_handler_returns_ok() {
        let fixture = TestDir::new("scan-handler");
        let state = AppState {
            movie_directory: fixture.0.clone(),
            port: 8081,
            depth: 0,
            index_cache: Arc::new(RwLock::new(Vec::new())),
            thumbnails: None,
            scan_lock: Arc::new(Mutex::new(())),
        };

        let status = refresh_index_handler(State(state)).await;
        assert_eq!(status, StatusCode::OK);
    }
}
