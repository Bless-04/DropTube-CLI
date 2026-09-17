//! Opt-in FFmpeg thumbnail generation. Callers serialize scans; FFmpeg runs asynchronously.

use crate::droptube_dir;
use crate::utils::prepare_droptube_directory;
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
    pub const GENERATED_PATH: &str = droptube_dir!("/thumbnails");

    /// FFmpeg configuration args
    pub const CONFIG_ARGS: [&'static str; 7] = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-threads",
        "1",
        "-i",
    ];

    /// FFmpeg Args to generate the thumbnail
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
    /// The cache lives beside the video in `.droptube/thumbnails`, with the full
    /// video filename retained to distinguish containers with the same stem.
    /// Its `.droptube` directory is dot-hidden on Unix and marked Hidden on
    /// Windows, including existing data directories encountered during generation.
    /// Callers must serialize generation for the same source. A failed or cancelled
    /// decode never publishes a partial JPEG; timed-out/dropped children are killed.
    pub async fn generate_thumbnail(&self, source: &Path) -> io::Result<PathBuf> {
        let source = fs::canonicalize(source).await?;
        let destination = thumbnail_path(&source)?;
        let source_directory = source
            .parent()
            .ok_or_else(|| io::Error::other("video has no parent directory"))?;
        let source_modified = fs::metadata(&source).await?.modified()?;
        if let Ok(metadata) = fs::metadata(&destination).await
            && metadata.is_file()
            && metadata.len() > 0
            && metadata.modified()? >= source_modified
        {
            prepare_thumbnail_directory(source_directory).await?;
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
        prepare_thumbnail_directory(source_directory).await?;
        let temporary = destination.with_extension("jpg.tmp");
        fs::write(&temporary, output.stdout).await?;
        fs::rename(&temporary, &destination).await?;
        Ok(destination)
    }
}

async fn prepare_thumbnail_directory(parent: &Path) -> io::Result<()> {
    prepare_droptube_directory(parent).await?;
    fs::create_dir_all(parent.join(ThumbnailGenerator::GENERATED_PATH)).await
}

async fn run_command(command: &mut Command, limit: Duration) -> io::Result<Output> {
    command.stdin(Stdio::null()).kill_on_drop(true);
    // Avoid console windows when the server is launched as a desktop/background process.
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    timeout(limit, command.output())
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Child process timed out"))?
}

fn thumbnail_path(source: &Path) -> io::Result<PathBuf> {
    let parent = source
        .parent()
        .ok_or_else(|| io::Error::other("video has no parent directory"))?;
    let mut name = source
        .file_name()
        .ok_or_else(|| io::Error::other("video has no filename"))?
        .to_os_string();
    name.push(".jpg");
    Ok(parent.join(ThumbnailGenerator::GENERATED_PATH).join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn test_generated_path_hidden_on_unix() {
        assert!(ThumbnailGenerator::GENERATED_PATH.starts_with('.')) // stuff starting with '.' are auto hidden on unix based systems
    }
    #[test]
    fn cache_paths_preserve_container_and_special_characters() {
        assert_eq!(
            thumbnail_path(Path::new("movies/a & b.mp4")).expect("path"),
            PathBuf::from(format!(
                "movies/{}/a & b.mp4.jpg",
                ThumbnailGenerator::GENERATED_PATH
            ))
        );

        assert_ne!(
            thumbnail_path(Path::new("movies/a.mp4")).expect("path"),
            thumbnail_path(Path::new("movies/a.mkv")).expect("path")
        );
        assert!(thumbnail_path(Path::new("")).is_err());
    }

    #[tokio::test]
    async fn prepares_thumbnails_under_the_droptube_directory() {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir().join(format!(
            "droptube-thumbnail-directory-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&parent).await.expect("create test parent");

        prepare_thumbnail_directory(&parent)
            .await
            .expect("prepare thumbnail directory");
        let droptube_directory = parent.join(crate::utils::DROPTUBE_DIRECTORY);
        let thumbnail_directory = droptube_directory.join("thumbnails");
        assert!(
            fs::metadata(&thumbnail_directory)
                .await
                .expect("thumbnail directory metadata")
                .is_dir()
        );

        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            assert_ne!(
                fs::metadata(&droptube_directory)
                    .await
                    .expect("DropTube directory metadata")
                    .file_attributes()
                    & 0x2,
                0,
                ".droptube must have the Hidden attribute"
            );
            assert_eq!(
                fs::metadata(&thumbnail_directory)
                    .await
                    .expect("thumbnail directory metadata")
                    .file_attributes()
                    & 0x2,
                0,
                "the thumbnails child directory must not have the Hidden attribute"
            );
        }

        fs::remove_dir_all(parent)
            .await
            .expect("remove test parent");
    }

    #[tokio::test]
    async fn missing_ffmpeg_returns_an_error() {
        let result = ThumbnailGenerator::new(PathBuf::from("/droptube-missing-tools/ffmpeg")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn an_unrelated_executable_is_rejected() {
        let executable = std::env::current_exe().expect("test executable");
        assert!(ThumbnailGenerator::new(executable).await.is_err());
    }

    #[tokio::test]
    async fn missing_video_returns_an_error_before_decoding() {
        let generator = ThumbnailGenerator {
            executable: PathBuf::from("unused"),
        };
        assert!(
            generator
                .generate_thumbnail(Path::new("/droptube-missing-videos/no.mp4"))
                .await
                .is_err()
        );
    }
}
