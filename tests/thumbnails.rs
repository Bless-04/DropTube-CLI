//! Filesystem and real-FFmpeg regression tests for opt-in thumbnail generation.
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
}
#[tokio::test]
async fn refresh_waits_for_the_scan_lock_and_publishes_new_files() {
}
#[tokio::test]
#[ignore = "requires FFmpeg: cargo test --test thumbnails -- --ignored"]
async fn real_ffmpeg_generates_reuses_invalidates_and_handles_corrupt_videos() {
    );
}
