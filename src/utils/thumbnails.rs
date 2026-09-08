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
}
