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
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn enabling_generation_panics_at_startup_if_ffmpeg_is_missing() {
    let library = Library::new();
    let fake_ffmpeg = library.0.join("fake_ffmpeg.exe");
    fs::write(&fake_ffmpeg, b"not really ffmpeg").expect("write fake ffmpeg binary");

    let output = Command::new(env!("CARGO_BIN_EXE_droptube"))
        .arg(&library.0)
        .arg("--use-thumbnails")
        .arg("--ffmpeg-path")
        .arg(&fake_ffmpeg)
        .output()
        .expect("start droptube");
    assert!(
        !output.status.success(),
        "Server startup should fail when thumbnail generation is enabled with an invalid FFmpeg binary"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FFmpeg"),
        "Error output should mention FFmpeg requirement: {stderr}"
    );
}

#[test]
fn startup_fails_when_ffmpeg_missing_from_path() {
    let library = Library::new();
    let output = Command::new(env!("CARGO_BIN_EXE_droptube"))
        .arg(&library.0)
        .arg("--use-thumbnails")
        .env("PATH", "")
        .output()
        .expect("start droptube");
    assert!(
        !output.status.success(),
        "Server startup should fail when --use-thumbnails is passed but FFmpeg is missing from PATH"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FFmpeg"),
        "Error output should mention FFmpeg when missing from PATH: {stderr}"
    );
}

#[test]
fn startup_fails_when_ffmpeg_path_arg_does_not_exist() {
    let library = Library::new();
    let output = Command::new(env!("CARGO_BIN_EXE_droptube"))
        .arg(&library.0)
        .arg("--use-thumbnails")
        .arg("--ffmpeg-path")
        .arg(library.0.join("missing-ffmpeg-binary"))
        .output()
        .expect("start droptube");
    assert!(
        !output.status.success(),
        "Server startup should fail when --ffmpeg-path points to a non-existent binary"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not exist"),
        "Error output should state that the FFmpeg binary does not exist: {stderr}"
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
    assert_eq!(
        index.len(),
        1,
        "Library index cache should contain exactly 1 scanned video"
    );
    assert_eq!(
        index[0].thumbnail_path.as_deref(),
        Some("first.jpg"),
        "Video entry should preserve the existing sidecar thumbnail"
    );
    assert!(
        !library.0.join(ThumbnailGenerator::GENERATED_PATH).exists(),
        "Generated thumbnail directory should not exist when thumbnail generation is disabled"
    );
    assert_eq!(
        fs::read(library.0.join("first.jpg")).expect("read sidecar"),
        b"custom image",
        "Existing sidecar image file must not be modified or overwritten"
    );
}

#[tokio::test]
async fn refresh_waits_for_the_scan_lock_and_publishes_new_files() {
    let library = Library::new();
    let state = library.state(None);
    let guard = state.scan_lock.lock().await;
    let refresh = state.refresh_index();
    tokio::pin!(refresh);
    assert!(
        matches!(
            std::future::poll_fn(|context| std::task::Poll::Ready(refresh.as_mut().poll(context)))
                .await,
            std::task::Poll::Pending
        ),
        "refresh_index must block while scan_lock is held by another task"
    );
    fs::write(library.0.join("new.mp4"), b"video").expect("write video");
    drop(guard);
    refresh.await.expect("refresh finishes");
    assert_eq!(
        state.index_cache.read().await.len(),
        1,
        "Index cache should reflect the newly added video once scan_lock is released"
    );
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
    let nested = library.0.join("nested & space");
    fs::create_dir(&nested).expect("nested directory");
    let source = nested.join("[action] O'Brien & demo.mp4");
    let output = Command::new(&executable)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=160x90:d=0.2",
            "-c:v",
            "mpeg4",
        ])
        .arg(&source)
        .output()
        .expect("create short fixture");
    assert!(
        output.status.success(),
        "Failed to generate test video fixture via FFmpeg: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let thumbnail = generator
        .generate_thumbnail(&source)
        .await
        .expect("generate short-video thumbnail");
    let image = fs::read(&thumbnail).expect("read JPEG");
    assert!(
        image.starts_with(&[0xff, 0xd8, 0xff]),
        "Generated thumbnail image must begin with JPEG SOI header (0xFF, 0xD8, 0xFF)"
    );
    assert!(
        image.ends_with(&[0xff, 0xd9]),
        "Generated thumbnail image must end with JPEG EOI marker (0xFF, 0xD9)"
    );
    let dimensions = Command::new(&executable)
        .args(["-hide_banner", "-i"])
        .arg(&thumbnail)
        .output()
        .expect("inspect image");
    assert!(
        String::from_utf8_lossy(&dimensions.stderr).contains("480x270"),
        "Generated thumbnail should have expected resolution 480x270"
    );
    let original_time = fs::metadata(&thumbnail)
        .expect("metadata")
        .modified()
        .expect("mtime");
    #[cfg(windows)]
    let original_directory_attributes = {
        use std::os::windows::fs::MetadataExt;
        let thumbnail_directory = thumbnail.parent().expect("thumbnail directory");
        let directory = thumbnail_directory
            .parent()
            .expect("DropTube data directory");
        let attributes = fs::metadata(directory)
            .expect("directory metadata")
            .file_attributes();
        assert_ne!(
            attributes & 0x2,
            0,
            "new .droptube directory must be Hidden"
        );
        assert_eq!(
            fs::metadata(thumbnail_directory)
                .expect("thumbnail directory metadata")
                .file_attributes()
                & 0x2,
            0,
            "thumbnails must not have the Hidden attribute"
        );
        // Simulate a visible data directory created by an older DropTube release.
        let status = Command::new("attrib.exe")
            .current_dir(directory.parent().expect("parent of .droptube"))
            .arg("-H")
            .arg(directory.file_name().expect("DropTube directory name"))
            .status()
            .expect("unhide fixture data directory");
        assert!(
            status.success(),
            "Command 'attrib -H' to unhide fixture data directory should succeed"
        );
        assert_eq!(
            fs::metadata(directory)
                .expect("directory metadata")
                .file_attributes()
                & 0x2,
            0,
            "Data directory should no longer have the Hidden attribute after attrib -H"
        );
        attributes
    };
    assert_eq!(
        generator
            .generate_thumbnail(&source)
            .await
            .expect("reuse thumbnail"),
        thumbnail,
        "Regenerating thumbnail for the same file should return the existing cached thumbnail path"
    );
    assert_eq!(
        fs::metadata(&thumbnail)
            .expect("metadata")
            .modified()
            .expect("mtime"),
        original_time,
        "Reusing an up-to-date cached thumbnail must not alter its file modification timestamp"
    );

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let directory = thumbnail
            .parent()
            .expect("thumbnail directory")
            .parent()
            .expect("DropTube data directory");
        assert_eq!(
            fs::metadata(directory)
                .expect("directory metadata")
                .file_attributes(),
            original_directory_attributes,
            "cache reuse must restore Hidden on .droptube without changing other attributes"
        );
    }

    // Deterministic stale-cache check, without sleep-based filesystem timing assumptions.
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
    File::options()
        .write(true)
        .open(&thumbnail)
        .expect("open cache")
        .set_times(FileTimes::new().set_modified(old))
        .expect("age cache");
    generator
        .generate_thumbnail(&source)
        .await
        .expect("regenerate stale thumbnail");
    assert!(
        fs::metadata(&thumbnail)
            .expect("metadata")
            .modified()
            .expect("mtime")
            > old,
        "Regenerating a stale thumbnail must update its file modification timestamp beyond the old time"
    );

    let corrupt = library.0.join("corrupt.mp4");
    fs::write(&corrupt, b"not a video").expect("write corrupt fixture");
    assert!(
        generator.generate_thumbnail(&corrupt).await.is_err(),
        "Thumbnail generator should return an error when processing a corrupted video file"
    );
    assert!(
        !library
            .0
            .join(format!(
                "{}/corrupt.mp4.jpg",
                ThumbnailGenerator::GENERATED_PATH
            ))
            .exists(),
        "No cached thumbnail image should be created on disk for a corrupted video file"
    );
    let sidecar = source.with_extension("jpg");
    fs::write(&sidecar, b"custom thumbnail").expect("write sidecar");
    let state = library.state(Some(generator));
    state
        .refresh_index()
        .await
        .expect("scan with corrupt video");
    let index = state.index_cache.read().await;
    assert_eq!(
        index.len(),
        2,
        "Library index should contain both valid and corrupt video entries"
    );
    assert!(
        index.iter().any(|video| video.thumbnail_path.as_deref()
            == Some("nested & space/[action] O'Brien & demo.jpg")),
        "Video with adjacent image sidecar should preserve its sidecar thumbnail path"
    );
    assert_eq!(
        fs::read(&sidecar).expect("sidecar retained"),
        b"custom thumbnail",
        "Existing custom sidecar image content must not be modified or overwritten"
    );
}
