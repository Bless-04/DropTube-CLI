use crate::models::video::VideoFile;
use crate::utils::scanner::{ScanDirectoryParams, scan_directory};
use crate::utils::thumbnails::ThumbnailGenerator;
use std::path::PathBuf;
use std::sync::Arc;
use std::{cmp::Reverse, io};
use tokio::sync::Mutex;
use tokio::sync::RwLock;

/// Shared application state passed to every Axum handler via [`axum::extract::State`].
#[derive(Clone)]
pub struct AppState {
    /// The canonical root directory being served.
    pub movie_directory: PathBuf,
    /// The port the server is listening on.
    pub port: u16,
    /// Maximum subfolder recursion depth (`0` = top-level only, `255` = unlimited).
    pub depth: u8,
    /// The in-memory video index, refreshed in the background every 30 seconds.
    pub index_cache: Arc<RwLock<Vec<VideoFile>>>,
    /// Optional, startup-validated FFmpeg generator. None means no generation or cache writes.
    pub thumbnails: Option<ThumbnailGenerator>,
    /// Serializes full scans so background and manual refreshes cannot race.
    pub scan_lock: Arc<Mutex<()>>,
}

impl AppState {
    /// Scans off the async runtime, generates missing thumbnails, then publishes the index.
    /// Existing sidecar images always take priority over generated thumbnails.
    pub async fn refresh_index(&self) -> io::Result<()> {
        let _scan_guard = self.scan_lock.lock().await;
        let directory = self.movie_directory.clone();
        let depth = self.depth;
        let mut videos = tokio::task::spawn_blocking(move || {
            let mut params = ScanDirectoryParams::new(directory, depth);
            scan_directory(&mut params);
            params.videos
        })
        .await
        .map_err(io::Error::other)?;
        videos.sort_by_key(|video| Reverse(video.unix_timestamp));
        if let Some(generator) = &self.thumbnails {
            for video in &mut videos {
                if video.thumbnail_path.is_some() {
                    continue;
                }
                match generator
                    .generate_thumbnail(&self.movie_directory.join(&video.file_name))
                    .await
                {
                    Ok(path) => match path.strip_prefix(&self.movie_directory) {
                        Ok(relative) => {
                            video.thumbnail_path =
                                Some(relative.to_string_lossy().replace('\\', "/"))
                        }
                        Err(error) => log::warn!("Thumbnail path is outside the library: {error}"),
                    },
                    Err(error) => log::warn!(
                        "Thumbnail generation failed for '{}': {error}",
                        video.file_name
                    ),
                }
            }
        }
        log::info!("Index refresh complete: {} video(s)", videos.len());
        *self.index_cache.write().await = videos;
        Ok(())
    }
}

/// Query parameters accepted by the home page handler.
#[derive(serde::Deserialize)]
pub struct HomeQuery {
    /// Optional video filename to auto-play on load.
    pub v: Option<String>,
    /// Current page number (1-indexed). Defaults to 1.
    pub page: Option<u32>,
    /// Search query to filter videos by display name.
    pub search: Option<String>,
    /// Tag filter to show only videos with this tag.
    pub tag: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File, FileTimes};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime};

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

        fn create_file(&self, relative: &str, content: &[u8]) -> PathBuf {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent dirs");
            }
            fs::write(&path, content).expect("write mock file");
            path
        }

        fn state(&self, depth: u8) -> AppState {
            AppState {
                movie_directory: self.0.clone(),
                port: 8081,
                depth,
                index_cache: Arc::new(RwLock::new(Vec::new())),
                thumbnails: None,
                scan_lock: Arc::new(Mutex::new(())),
            }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn refresh_index_populates_cache_and_sorts_by_timestamp_descending() {
        let fixture = TestDir::new("state-sort");
        let v1_path = fixture.create_file("older.mp4", b"video1");
        let v2_path = fixture.create_file("newer.mp4", b"video2");

        let old_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let new_time = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000);

        File::options()
            .write(true)
            .open(&v1_path)
            .expect("open v1")
            .set_times(FileTimes::new().set_modified(old_time))
            .expect("set time v1");

        File::options()
            .write(true)
            .open(&v2_path)
            .expect("open v2")
            .set_times(FileTimes::new().set_modified(new_time))
            .expect("set time v2");

        let state = fixture.state(255);
        state.refresh_index().await.expect("refresh index");

        let index = state.index_cache.read().await;
        assert_eq!(index.len(), 2);
        assert_eq!(index[0].file_name, "newer.mp4");
        assert_eq!(index[1].file_name, "older.mp4");
    }

    #[tokio::test]
    async fn refresh_index_preserves_sidecars_without_generator() {
        let fixture = TestDir::new("state-sidecar");
        fixture.create_file("show.mp4", b"video");
        fixture.create_file("show.jpg", b"image-sidecar");

        let state = fixture.state(0);
        state.refresh_index().await.expect("refresh index");

        let index = state.index_cache.read().await;
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].thumbnail_path.as_deref(), Some("show.jpg"));
    }

    #[tokio::test]
    async fn refresh_index_respects_max_depth() {
        let fixture = TestDir::new("state-depth");
        fixture.create_file("root.mp4", b"video");
        fixture.create_file("sub/child.mp4", b"video");

        // max_depth = 0: only root
        let state0 = fixture.state(0);
        state0.refresh_index().await.expect("refresh index");
        assert_eq!(state0.index_cache.read().await.len(), 1);

        // max_depth = 1: root and sub
        let state1 = fixture.state(1);
        state1.refresh_index().await.expect("refresh index");
        assert_eq!(state1.index_cache.read().await.len(), 2);
    }

    #[tokio::test]
    async fn concurrent_refresh_calls_are_safe() {
        let fixture = TestDir::new("state-concurrent");
        for i in 0..5 {
            fixture.create_file(&format!("video_{i}.mp4"), b"video");
        }

        let state = fixture.state(255);

        // Spawn multiple concurrent refresh calls
        let (r1, r2, r3) = tokio::join!(
            state.refresh_index(),
            state.refresh_index(),
            state.refresh_index(),
        );

        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert!(r3.is_ok());

        assert_eq!(state.index_cache.read().await.len(), 5);
    }
}

