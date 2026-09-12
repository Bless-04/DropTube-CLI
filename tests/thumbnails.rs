//! Filesystem and real-FFmpeg regression tests for opt-in thumbnail generation.

use droptube::models::state::AppState;
use droptube::utils::thumbnails::ThumbnailGenerator;
use std::fs::{self, File, FileTimes};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, SystemTime};
use tokio::sync::{Mutex, RwLock};

struct Library(PathBuf);

impl Library {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "droptube-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create isolated test directory");
        Self(path.canonicalize().expect("canonical test directory"))
    }

    fn state(&self, thumbnails: Option<ThumbnailGenerator>) -> AppState {
        AppState {
            movie_directory: self.0.clone(),
            port: 0,
            depth: 255,
            index_cache: Arc::new(RwLock::new(Vec::new())),
            thumbnails,
            scan_lock: Arc::new(Mutex::new(())),
        }
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove isolated test directory");
    }
}

#[test]
fn enabling_generation_panics_at_startup_if_ffmpeg_is_missing() {
    let library = Library::new();
    let output = Command::new(env!("CARGO_BIN_EXE_droptube"))
        .arg(&library.0)
        .arg("--generate-thumbnails")
        .arg("--ffmpeg-path")
        .arg(library.0.join("not-installed-ffmpeg"))
        .output()
        .expect("start droptube");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("System panic detected"), "{stderr}");
    assert!(
        stderr.contains("--generate-thumbnails requires FFmpeg"),
        "{stderr}"
    );
}

#[tokio::test]
async fn generation_is_disabled_by_default_and_sidecars_are_preserved() {
    let library = Library::new();
    fs::write(library.0.join("first.mp4"), b"video").expect("write video");
    fs::write(library.0.join("first.jpg"), b"custom image").expect("write sidecar");
    let state = library.state(None);
    state.refresh_index().await.expect("scan");
    let index = state.index_cache.read().await;
    assert_eq!(index.len(), 1);
    assert_eq!(index[0].thumbnail_path.as_deref(), Some("first.jpg"));
    assert!(!library.0.join(ThumbnailGenerator::GENERATED_PATH).exists());
    assert_eq!(
        fs::read(library.0.join("first.jpg")).expect("read sidecar"),
        b"custom image"
    );
}

#[tokio::test]
async fn refresh_waits_for_the_scan_lock_and_publishes_new_files() {
    let library = Library::new();
    let state = library.state(None);
    let guard = state.scan_lock.lock().await;
    let refresh = state.refresh_index();
    tokio::pin!(refresh);
    assert!(matches!(
        std::future::poll_fn(|context| std::task::Poll::Ready(refresh.as_mut().poll(context)))
            .await,
        std::task::Poll::Pending
    ));
    fs::write(library.0.join("new.mp4"), b"video").expect("write video");
    drop(guard);
    refresh.await.expect("refresh finishes");
    assert_eq!(state.index_cache.read().await.len(), 1);
}

/// Run with `DROPTUBE_TEST_FFMPEG` set to an FFmpeg executable, or FFmpeg on PATH.
#[tokio::test]
#[ignore = "requires FFmpeg: cargo test --test thumbnails -- --ignored"]
async fn real_ffmpeg_generates_reuses_invalidates_and_handles_corrupt_videos() {
    let library = Library::new();
    let executable = std::env::var_os("DROPTUBE_TEST_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "ffmpeg".into());
    let generator = ThumbnailGenerator::new(executable.clone())
        .await
        .expect("working FFmpeg");
    );
}
