use crate::config::constants::DROPTUBE_DIRECTORY;
use crate::models::video::{SubtitleFormat, SubtitleTrack, VideoFile, VideoFormat};
use log::warn;
use std::fs;
use std::path::{Path as StdPath, PathBuf};
use std::time::SystemTime;

/// Display information derived from a video filename, without personal metadata.
pub struct ParsedVideoInfo {
    /// Human-readable display name, with underscores/dots replaced by spaces.
    pub display_name: String,
}

/// Formats a filename's stem for display, keeping bracketed prefixes as title text.
///
/// Underscores and dots are replaced with spaces. Ratings and tags are managed
/// exclusively through profiles and are never interpreted from a filename.
///
/// ```
/// use droptube::utils::scanner::parse_video_info;
/// use std::path::Path;
/// assert_eq!(parse_video_info(Path::new("[5] Movie_Name.mp4")).display_name, "[5] Movie Name");
/// ```
pub fn parse_video_info(path: &StdPath) -> ParsedVideoInfo {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown Video".to_string());

    ParsedVideoInfo {
        display_name: stem.replace(['_', '.'], " "),
    }
}

/// Parameters controlling a single `scan_directory` invocation.
pub struct ScanDirectoryParams {
    /// The directory to read entries from in this call.
    pub current_dir: PathBuf,
    /// The root directory the scan started from; used to compute web-relative paths.
    pub base_dir: PathBuf,
    /// Maximum recursion depth (`0` = top-level only, `u8::MAX` = unlimited).
    pub max_depth: u8,
    /// The recursion level of *this* call (root call starts at `0`).
    pub current_depth: u8,
    /// Running count of all filesystem entries examined (files + dirs).
    pub count: usize,
    /// Accumulated list of discovered video files.
    pub videos: Vec<VideoFile>,
}

impl ScanDirectoryParams {
    /// Image extensions checked when looking for sidecar thumbnails.
    pub const THUMBNAIL_EXT: [&'static str; 4] = ["jpg", "jpeg", "png", "webp"];

    /// Creates a root-level scan starting at `root_dir` with the given `max_depth`.
    pub fn new(root_dir: PathBuf, max_depth: u8) -> Self {
        Self {
            base_dir: root_dir.clone(),
            current_dir: root_dir,
            max_depth,
            current_depth: 0,
            count: 0,
            videos: Vec::new(),
        }
    }

    /// Returns `true` if the scanner should recurse into subdirectories from the current depth.
    fn should_recurse(&self) -> bool {
        self.max_depth == u8::MAX || self.current_depth < self.max_depth
    }
}

/// Maps an optional subtitle language code to a user-facing label.
fn subtitle_language_label(lang: &str) -> String {
    match lang.to_lowercase().as_str() {
        "en" | "eng" => "English".to_string(),
        "en-us" => "English (US)".to_string(),
        "en-gb" => "English (UK)".to_string(),
        "es" | "spa" => "Spanish".to_string(),
        "fr" | "fra" | "fre" => "French".to_string(),
        "de" | "deu" | "ger" => "German".to_string(),
        "it" | "ita" => "Italian".to_string(),
        "pt" | "por" => "Portuguese".to_string(),
        "pt-br" => "Portuguese (BR)".to_string(),
        "ja" | "jpn" => "Japanese".to_string(),
        "zh" | "zho" | "chi" => "Chinese".to_string(),
        "ko" | "kor" => "Korean".to_string(),
        "ru" | "rus" => "Russian".to_string(),
        "ar" | "ara" => "Arabic".to_string(),
        "hi" | "hin" => "Hindi".to_string(),
        _ => lang.to_string(),
    }
}

/// Discovers sidecar subtitle files (.vtt or .srt) matching a video file in the same directory.
fn discover_subtitles(
    candidates: &[PathBuf],
    video_stem: &str,
    base_dir: &StdPath,
) -> Vec<SubtitleTrack> {
    if video_stem.is_empty() {
        return Vec::new();
    }
    let prefix = format!("{video_stem}.");
    let mut tracks = Vec::new();

    for candidate in candidates {
        let Some(ext) = candidate.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(format) = SubtitleFormat::from_ext(ext) else {
            continue;
        };
        let Some(cand_stem) = candidate.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };

        let (language, label) = if cand_stem == video_stem {
            (None, "Subtitles".to_string())
        } else if cand_stem.starts_with(&prefix) {
            let lang = &cand_stem[prefix.len()..];
            if lang.is_empty() {
                continue;
            }
            (Some(lang.to_string()), subtitle_language_label(lang))
        } else {
            continue;
        };

        match fs::metadata(candidate) {
            Ok(_) => {
                let rel_path = candidate
                    .strip_prefix(base_dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| candidate.to_string_lossy().to_string())
                    .replace('\\', "/");
                tracks.push(SubtitleTrack {
                    relative_path: rel_path,
                    format,
                    language,
                    label,
                });
            }
            Err(err) => {
                warn!(
                    "Failed to retrieve metadata for subtitle sidecar '{}': {}",
                    candidate.display(),
                    err
                );
            }
        }
    }

    tracks.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    tracks
}

/// Recursively (or non-recursively, depending on `params.max_depth`) scans
/// `params.current_dir` for supported video files.
///
/// Results are accumulated in `params.videos`; the total item count in `params.count`.
pub fn scan_directory(params: &mut ScanDirectoryParams) {
    let current_dir = &params.current_dir;
    let base_dir = &params.base_dir;

    let entries = match fs::read_dir(current_dir) {
        Ok(e) => e,
        Err(err) => {
            warn!(
                "Failed to read directory '{}': {}",
                current_dir.display(),
                err
            );
            return;
        }
    };

    let mut sub_dirs = Vec::new();
    let mut dir_files = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(error = %error, "Could not read a library directory entry");
                continue;
            }
        };
        let e_path = entry.path();

        // Skip cache and links: linked directories can loop or leave the library.
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(DROPTUBE_DIRECTORY))
            || entry.file_type().is_ok_and(|kind| kind.is_symlink())
        {
            continue;
        }

        if e_path.is_dir() {
            sub_dirs.push(e_path);
        } else if e_path.is_file() {
            dir_files.push(e_path);
        }
    }

    if params.should_recurse() {
        for sub_dir in sub_dirs {
            let mut sub_params = ScanDirectoryParams {
                current_dir: sub_dir,
                base_dir: base_dir.clone(),
                max_depth: params.max_depth,
                current_depth: params.current_depth.saturating_add(1),
                count: 0,
                videos: Vec::new(),
            };
            scan_directory(&mut sub_params);
            params.count = params.count.saturating_add(sub_params.count);
            params.videos.extend(sub_params.videos);
        }
    }

    for e_path in &dir_files {
        params.count = params.count.saturating_add(1);

        let Some(ext) = e_path.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(format) = VideoFormat::from_ext(ext) else {
            continue;
        };

        // Compute web-friendly relative path from base_dir for the streaming URL
        let file_name = e_path
            .strip_prefix(base_dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| e_path.to_string_lossy().to_string())
            .replace('\\', "/");

        let video_info = parse_video_info(e_path);

        // Look for a sidecar thumbnail (e.g. `video.jpg` alongside `video.mp4`)
        let mut thumbnail_path = None;
        for img_ext in &ScanDirectoryParams::THUMBNAIL_EXT {
            let mut test_thumb = e_path.clone();
            test_thumb.set_extension(img_ext);
            if test_thumb.exists() && test_thumb.is_file() {
                let rel_thumb = test_thumb
                    .strip_prefix(base_dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| test_thumb.to_string_lossy().to_string())
                    .replace('\\', "/");
                thumbnail_path = Some(rel_thumb);
                break;
            }
        }

        let video_stem = e_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let subtitles = discover_subtitles(&dir_files, video_stem, base_dir);

        match fs::metadata(e_path) {
            Ok(metadata) => {
                let file_size_mb = metadata.len() / (1024 * 1024);
                let unix_timestamp = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                params.videos.push(VideoFile {
                    file_name,
                    display_name: video_info.display_name,
                    file_size_mb,
                    unix_timestamp,
                    format,
                    thumbnail_path,
                    subtitles,
                });
            }
            Err(err) => {
                warn!(
                    "Failed to retrieve metadata for '{}': {}",
                    e_path.display(),
                    err
                );
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_names_without_interpreting_bracketed_metadata() {
        for (filename, title) in [
            ("my_cool_video.mp4", "my cool video"),
            ("[4] Awesome_Movie.mkv", "[4] Awesome Movie"),
            ("[rust,tech] My_Talk.mp4", "[rust,tech] My Talk"),
            (
                "[5] [action,drama] Movie.2024.mp4",
                "[5] [action,drama] Movie 2024",
            ),
            ("[invalid] incomplete_[.mp4", "[invalid] incomplete ["),
            ("Film_映画.mp4", "Film 映画"),
        ] {
            assert_eq!(parse_video_info(StdPath::new(filename)).display_name, title);
        }
    }

    #[test]
    fn path_without_a_stem_uses_an_explicit_unknown_title() {
        assert_eq!(
            parse_video_info(StdPath::new("/")).display_name,
            "Unknown Video"
        );
    }

    #[test]
    fn scan_params_should_recurse_at_limit() {
        let p = ScanDirectoryParams::new(PathBuf::from("."), 2);
        // depth 0, max 2 — should recurse
        assert!(
            p.should_recurse(),
            "ScanDirectoryParams with current_depth 0 and max_depth 2 should allow recursion"
        );
    }

    #[test]
    fn scan_params_no_recurse_when_depth_zero() {
        let mut p = ScanDirectoryParams::new(PathBuf::from("."), 0);
        p.current_depth = 0;
        // max_depth 0 means top-level only — must NOT recurse
        assert!(
            !p.should_recurse(),
            "ScanDirectoryParams with max_depth 0 should NOT recurse into subdirectories"
        );
    }

    #[test]
    fn scan_params_unlimited_always_recurses() {
        let mut p = ScanDirectoryParams::new(PathBuf::from("."), u8::MAX);
        p.current_depth = 100;
        assert!(
            p.should_recurse(),
            "ScanDirectoryParams with max_depth u8::MAX should always allow recursion"
        );
    }

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(prefix: &str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "droptube-{prefix}-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("create test dir");
            Self(path)
        }

        fn create_file(&self, relative_path: &str, content: &[u8]) -> PathBuf {
            let file_path = self.0.join(relative_path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).expect("create parent dirs");
            }
            std::fs::write(&file_path, content).expect("write mock file");
            file_path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn scan_directory_empty_folder() {
        let fixture = TestDir::new("scan-empty");
        let mut params = ScanDirectoryParams::new(fixture.0.clone(), 255);
        scan_directory(&mut params);
        assert!(
            params.videos.is_empty(),
            "Scanning an empty directory should return no videos"
        );
        assert_eq!(
            params.count, 0,
            "Total examined file count should be 0 for an empty folder"
        );
    }

    #[test]
    fn scan_directory_detects_all_supported_video_formats_and_ignores_non_videos() {
        let fixture = TestDir::new("scan-formats");
        for ext in VideoFormat::SUPPORTED_EXTS {
            fixture.create_file(&format!("video.{ext}"), b"dummy-video-data");
        }
        // Unsupported video formats
        fixture.create_file("unsupported.flv", b"flv");
        fixture.create_file("unsupported.wmv", b"wmv");
        // Non-video files
        fixture.create_file("notes.txt", b"notes");
        fixture.create_file("audio.mp3", b"music");
        fixture.create_file("document.pdf", b"pdf");

        let mut params = ScanDirectoryParams::new(fixture.0.clone(), 0);
        scan_directory(&mut params);

        assert_eq!(
            params.videos.len(),
            VideoFormat::SUPPORTED_EXTS.len(),
            "Scanner should only collect files matching supported video extensions"
        );
        // count includes all files examined (6 supported + 2 unsupported + 3 non-video = 11)
        assert_eq!(
            params.count,
            VideoFormat::SUPPORTED_EXTS.len() + 5,
            "Total examined file count should include both video and non-video files"
        );

        for ext in VideoFormat::SUPPORTED_EXTS {
            assert!(
                params
                    .videos
                    .iter()
                    .any(|v| v.file_name == format!("video.{ext}")),
                "Missing supported video format in scan results: {ext}"
            );
        }
    }

    #[test]
    fn scan_directory_respects_depth_limits() {
        let fixture = TestDir::new("scan-depth");
        fixture.create_file("root.mp4", b"data");
        fixture.create_file("sub1/level1.mp4", b"data");
        fixture.create_file("sub1/sub2/level2.mp4", b"data");
        fixture.create_file("sub1/sub2/sub3/level3.mp4", b"data");

        // Depth 0: only root
        let mut p0 = ScanDirectoryParams::new(fixture.0.clone(), 0);
        scan_directory(&mut p0);
        assert_eq!(
            p0.videos.len(),
            1,
            "Depth 0 scan should only collect root-level videos"
        );
        assert_eq!(
            p0.videos[0].file_name, "root.mp4",
            "Root-level video should be 'root.mp4'"
        );

        // Depth 1: root + level1
        let mut p1 = ScanDirectoryParams::new(fixture.0.clone(), 1);
        scan_directory(&mut p1);
        assert_eq!(
            p1.videos.len(),
            2,
            "Depth 1 scan should collect root and level 1 videos"
        );
        assert!(
            p1.videos.iter().any(|v| v.file_name == "root.mp4"),
            "Depth 1 results should include root.mp4"
        );
        assert!(
            p1.videos.iter().any(|v| v.file_name.contains("level1.mp4")),
            "Depth 1 results should include level1.mp4"
        );

        // Depth 2: root + level1 + level2
        let mut p2 = ScanDirectoryParams::new(fixture.0.clone(), 2);
        scan_directory(&mut p2);
        assert_eq!(
            p2.videos.len(),
            3,
            "Depth 2 scan should collect videos down to level 2"
        );

        // Unlimited depth (255): all 4
        let mut p_all = ScanDirectoryParams::new(fixture.0.clone(), 255);
        scan_directory(&mut p_all);
        assert_eq!(
            p_all.videos.len(),
            4,
            "Unlimited depth scan should collect all 4 nested videos"
        );
    }

    #[test]
    fn scan_directory_pairs_sidecar_thumbnails() {
        let fixture = TestDir::new("scan-sidecars");
        fixture.create_file("film1.mp4", b"data");
        fixture.create_file("film1.jpg", b"jpeg-thumbnail");

        fixture.create_file("film2.mkv", b"data");
        fixture.create_file("film2.png", b"png-thumbnail");

        fixture.create_file("film3.webm", b"data"); // no thumbnail

        let mut params = ScanDirectoryParams::new(fixture.0.clone(), 0);
        scan_directory(&mut params);

        let v1 = params
            .videos
            .iter()
            .find(|v| v.file_name == "film1.mp4")
            .unwrap();
        assert_eq!(
            v1.thumbnail_path.as_deref(),
            Some("film1.jpg"),
            "film1.mp4 should be paired with adjacent sidecar thumbnail film1.jpg"
        );

        let v2 = params
            .videos
            .iter()
            .find(|v| v.file_name == "film2.mkv")
            .unwrap();
        assert_eq!(
            v2.thumbnail_path.as_deref(),
            Some("film2.png"),
            "film2.mkv should be paired with adjacent sidecar thumbnail film2.png"
        );

        let v3 = params
            .videos
            .iter()
            .find(|v| v.file_name == "film3.webm")
            .unwrap();
        assert_eq!(
            v3.thumbnail_path, None,
            "film3.webm has no adjacent sidecar and should have None thumbnail_path"
        );
    }

    #[test]
    fn scan_directory_ignores_droptube_internal_directory() {
        let fixture = TestDir::new("scan-droptube-dir");
        fixture.create_file("video.mp4", b"video");
        // Put a video inside .droptube
        fixture.create_file(
            &format!("{DROPTUBE_DIRECTORY}/thumbnails/cached.mp4"),
            b"cached",
        );

        let mut params = ScanDirectoryParams::new(fixture.0.clone(), 255);
        scan_directory(&mut params);

        assert_eq!(
            params.videos.len(),
            1,
            "Scanner must ignore videos stored inside the internal .droptube directory"
        );
        assert_eq!(
            params.videos[0].file_name, "video.mp4",
            "Only user videos outside .droptube should be discovered"
        );
    }

    #[test]
    fn scan_directory_preserves_bracketed_titles_ids_and_thumbnail_matching() {
        let fixture = TestDir::new("scan-meta");
        fixture.create_file(
            "tutorials/rust/[5] [rust,tech] Advanced_Patterns.2024.mp4",
            b"video-bytes",
        );

        fixture.create_file(
            "tutorials/rust/[5] [rust,tech] Advanced_Patterns.2024.jpg",
            b"thumbnail",
        );

        let mut params = ScanDirectoryParams::new(fixture.0.clone(), 255);
        scan_directory(&mut params);

        assert_eq!(
            params.videos.len(),
            1,
            "Scanner should find exactly 1 video in subdirectories"
        );
        let video = &params.videos[0];
        assert_eq!(
            video.display_name, "[5] [rust,tech] Advanced Patterns 2024",
            "Bracketed prefixes remain ordinary text while dots and underscores are formatted"
        );
        assert_eq!(
            video.thumbnail_path.as_deref(),
            Some("tutorials/rust/[5] [rust,tech] Advanced_Patterns.2024.jpg")
        );
        // Verify web-friendly forward slashes on all platforms
        assert!(
            !video.file_name.contains('\\'),
            "Scanned relative file path must use forward slashes for cross-platform web URL compatibility"
        );
        assert_eq!(
            video.file_name, "tutorials/rust/[5] [rust,tech] Advanced_Patterns.2024.mp4",
            "Scanned file_name should match the expected relative path with forward slashes"
        );
    }

    #[test]
    fn scan_directory_pairs_sidecar_subtitles() {
        let fixture = TestDir::new("scan-subtitles");
        fixture.create_file("movies/interstellar.mp4", b"video");
        fixture.create_file("movies/interstellar.vtt", b"WEBVTT");
        fixture.create_file("movies/interstellar.en.vtt", b"WEBVTT-en");
        fixture.create_file(
            "movies/interstellar.es.srt",
            b"1\n00:00:01,000 --> 00:00:02,000\nHola",
        );
        fixture.create_file("movies/interstellar.de-DE.vtt", b"WEBVTT-de");
        fixture.create_file("movies/interstellar-trailer.vtt", b"trailer");
        fixture.create_file("movies/other.mkv", b"video2");
        fixture.create_file(
            "movies/other.srt",
            b"1\n00:00:01,000 --> 00:00:02,000\nOther",
        );

        let mut params = ScanDirectoryParams::new(fixture.0.clone(), 255);
        scan_directory(&mut params);

        let interstellar = params
            .videos
            .iter()
            .find(|v| v.file_name == "movies/interstellar.mp4")
            .expect("interstellar.mp4 exists");

        assert_eq!(interstellar.subtitles.len(), 4);
        assert_eq!(
            interstellar.subtitles[0].relative_path,
            "movies/interstellar.de-DE.vtt"
        );
        assert_eq!(interstellar.subtitles[0].format, SubtitleFormat::Vtt);
        assert_eq!(interstellar.subtitles[0].language.as_deref(), Some("de-DE"));
        assert_eq!(interstellar.subtitles[0].label, "de-DE");

        assert_eq!(
            interstellar.subtitles[1].relative_path,
            "movies/interstellar.en.vtt"
        );
        assert_eq!(interstellar.subtitles[1].format, SubtitleFormat::Vtt);
        assert_eq!(interstellar.subtitles[1].language.as_deref(), Some("en"));
        assert_eq!(interstellar.subtitles[1].label, "English");

        assert_eq!(
            interstellar.subtitles[2].relative_path,
            "movies/interstellar.es.srt"
        );
        assert_eq!(interstellar.subtitles[2].format, SubtitleFormat::Srt);
        assert_eq!(interstellar.subtitles[2].language.as_deref(), Some("es"));
        assert_eq!(interstellar.subtitles[2].label, "Spanish");

        assert_eq!(
            interstellar.subtitles[3].relative_path,
            "movies/interstellar.vtt"
        );
        assert_eq!(interstellar.subtitles[3].format, SubtitleFormat::Vtt);
        assert_eq!(interstellar.subtitles[3].language, None);
        assert_eq!(interstellar.subtitles[3].label, "Subtitles");

        let other = params
            .videos
            .iter()
            .find(|v| v.file_name == "movies/other.mkv")
            .expect("other.mkv exists");
        assert_eq!(other.subtitles.len(), 1);
        assert_eq!(other.subtitles[0].relative_path, "movies/other.srt");
        assert_eq!(other.subtitles[0].format, SubtitleFormat::Srt);
        assert_eq!(other.subtitles[0].language, None);
        assert_eq!(other.subtitles[0].label, "Subtitles");
    }
}
