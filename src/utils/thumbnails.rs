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
    ///
    /// The cache lives beside the video in `.droptube-thumbnails`, with the full
    /// video filename retained to distinguish containers with the same stem.
    /// Its directory is dot-hidden on Unix and marked Hidden on Windows, including
    /// existing cache directories encountered when reusing a thumbnail.
    /// Callers must serialize generation for the same source. A failed or cancelled
    /// decode never publishes a partial JPEG; timed-out/dropped children are killed.
    pub async fn generate_thumbnail(&self, source: &Path) -> io::Result<PathBuf> {
        let source = fs::canonicalize(source).await?;
        let destination = thumbnail_path(&source)?;
        let parent = destination
            .parent()
            .ok_or_else(|| io::Error::other("thumbnail has no parent directory"))?;
        let source_modified = fs::metadata(&source).await?.modified()?;
        if let Ok(metadata) = fs::metadata(&destination).await
            && metadata.is_file()
            && metadata.len() > 0
            && metadata.modified()? >= source_modified
        {
            prepare_thumbnail_directory(parent).await?;
            return Ok(destination);
        }

        let mut command = Command::new(&self.executable);
        command
            .args(ThumbnailGenerator::CONFIG_ARGS)
            .arg(&source)
            .args(ThumbnailGenerator::GENERATION_ARGS);
        let output = run_command(&mut command, Duration::from_secs(30)).await?;
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "FFmpeg could not decode the video: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        if !output.stdout.starts_with(&[0xff, 0xd8, 0xff]) // valid jpeg starting bytes; if it doesnt have this then something went wrong
            || !output.stdout.ends_with(&[0xff, 0xd9])
        {
            return Err(io::Error::other("FFmpeg produced no complete JPEG frame"));
        }
        prepare_thumbnail_directory(parent).await?;
        let temporary = destination.with_extension("jpg.tmp");
        fs::write(&temporary, output.stdout).await?;
        fs::rename(&temporary, &destination).await?;
        Ok(destination)
    }
}

//todo refactor this to use a lib to abstract this process away and for it to work the same across platforms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generated_path_hidden_on_unix() {
        assert!(ThumbnailGenerator::GENERATED_PATH.starts_with('.')) // stuff starting with '.' are auto hidden on unix based systems
    }
}
