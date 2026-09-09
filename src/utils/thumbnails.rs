//! Opt-in FFmpeg thumbnail generation. Callers serialize scans; FFmpeg runs asynchronously.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::{fs, process::Command, time::timeout};

/// A validated FFmpeg executable used to create cached JPEG thumbnails.
#[derive(Clone, Debug)]
pub struct ThumbnailGenerator {
    executable: PathBuf,
}

impl ThumbnailGenerator {
    /// The generated path for the thumbnails made by ffmpeg
    pub const GENERATED_PATH: &str = ".droptube-thumbnails";

    pub const GENERATION_ARGS: [&'static str; 19] = [
        "-map",
        "0:v:0",
        "-an",
        "-sn",
        "-filter_threads",
        "1",
        "-vf",
        "scale=480:270:force_original_aspect_ratio=decrease,pad=480:270:(ow-iw)/2:(oh-ih)/2,setsar=1,thumbnail=30",
        "-frames:v",
        "1",
        "-c:v",
        "mjpeg",
        "-threads",
        "1",
        "-q:v",
        "3",
        "-f",
        "image2pipe",
        "pipe:1",
    ];

    /// Checks that the executable starts and identifies itself as FFmpeg.
    /// Returns an error if it is absent, unusable, or takes more than five seconds.
    pub async fn new(executable: PathBuf) -> io::Result<Self> {
        let generator = Self { executable };
        let mut command = Command::new(&generator.executable);
        command.arg("-version");
        let output = run_command(&mut command, Duration::from_secs(5)).await?;
        if !output.status.success() || !output.stdout.starts_with(b"ffmpeg version ") {
            return Err(io::Error::other(
                "executable did not report a working FFmpeg version",
            ));
        }
        Ok(generator)
    }

    /// Creates a 480×270 JPEG, or reuses one newer than the source video.
    pub async fn generate_thumbnail(&self, source: &Path) -> io::Result<PathBuf> {
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generated_path_hidden_on_unix() {
        assert!(ThumbnailGenerator::GENERATED_PATH.starts_with('.')) // stuff starting with '.' are auto hidden on unix based systems
    }
}
