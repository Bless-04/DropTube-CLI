//! Filesystem and real-FFmpeg regression tests for opt-in thumbnail generation.
fn enabling_generation_panics_at_startup_if_ffmpeg_is_missing() {
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
