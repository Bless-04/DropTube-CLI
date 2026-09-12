use crate::models::video::{Rating, Tag, VideoFile, VideoFormat};
use crate::utils::DROPTUBE_DIRECTORY;
use log::warn;
use std::fs;
use std::path::{Path as StdPath, PathBuf};
use std::time::SystemTime;

/// Metadata parsed from a video filename (title, rating, tags).
pub struct ParsedVideoInfo {
    /// Human-readable display name, with underscores/dots replaced by spaces.
    pub display_name: String,
    /// Star rating encoded in the filename prefix, e.g. `[4]`.
    pub rating: Rating,
    /// Topic tags encoded in the filename, e.g. `[rust,tech]`.
    pub tags: Vec<Tag>,
}

/// Parses display name, rating, and tags from a video file path.
///
/// The convention (both fields are optional and order-sensitive):
/// 1. `[<1-5>] rest of name` — rating prefix
/// 2. `[tag1,tag2] rest of name` — tags prefix
///
/// Any underscores or dots in the final title are replaced with spaces.
pub fn parse_video_info(path: &StdPath) -> ParsedVideoInfo {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown Video".to_string());

    let mut title = stem;
    let mut rating = Rating::Unrated;
    let mut tags: Vec<Tag> = Vec::new();

    // Extract rating prefix: [<1-5>]
    if title.starts_with('[')
        && let Some(end_idx) = title.find(']')
    {
        let rating_val = title[1..end_idx].trim();
        if let Ok(r_num) = rating_val.parse::<u8>() {
            rating = Rating::from_u8(r_num);
            title = title[end_idx + 1..].trim().to_string();
        }
    }

    // Extract tags prefix: [tag1,tag2,...]
    if title.starts_with('[')
        && let Some(end_idx) = title.find(']')
    {
        let tags_val = &title[1..end_idx];
        tags = tags_val.split(',').map(Tag::parse).collect();
        title = title[end_idx + 1..].trim().to_string();
    }

    let display_name = title.replace(['_', '.'], " ");
    ParsedVideoInfo {
        display_name,
        rating,
        tags,
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

    for entry in entries.flatten() {
        let e_path = entry.path();

        // Skip cache and links: linked directories can loop or leave the library.
        if entry.file_name().eq(DROPTUBE_DIRECTORY)
            || entry.file_type().is_ok_and(|kind| kind.is_symlink())
        {
            continue;
        }

        if e_path.is_dir() {
            if params.should_recurse() {
                let mut sub_params = ScanDirectoryParams {
                    current_dir: e_path,
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
        } else if e_path.is_file() {
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

            let video_info = parse_video_info(&e_path);

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

            match fs::metadata(&e_path) {
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
                        rating: video_info.rating,
                        tags: video_info.tags,
                        thumbnail_path,
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
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_video_info_plain_name() {
        let path = StdPath::new("my_cool_video.mp4");
        let info = parse_video_info(path);
        assert_eq!(info.display_name, "my cool video");
        assert_eq!(info.rating, Rating::Unrated);
        assert!(info.tags.is_empty());
    }

    #[test]
    fn parse_video_info_with_rating() {
        let path = StdPath::new("[4] Awesome_Movie.mkv");
        let info = parse_video_info(path);
        assert_eq!(info.rating, Rating::FourStars);
        assert_eq!(info.display_name, "Awesome Movie");
    }

    #[test]
    fn parse_video_info_with_tags() {
        let path = StdPath::new("[rust,tech] My_Talk.mp4");
        let info = parse_video_info(path);
        assert!(info.tags.contains(&Tag::Rust));
        assert!(info.tags.contains(&Tag::Technology));
    }

    #[test]
    fn scan_params_should_recurse_at_limit() {
        let p = ScanDirectoryParams::new(PathBuf::from("."), 2);
        // depth 0, max 2 — should recurse
        assert!(p.should_recurse());
    }

    #[test]
    fn scan_params_no_recurse_when_depth_zero() {
        let mut p = ScanDirectoryParams::new(PathBuf::from("."), 0);
        p.current_depth = 0;
        // max_depth 0 means top-level only — must NOT recurse
        assert!(!p.should_recurse());
    }

    #[test]
    fn scan_params_unlimited_always_recurses() {
        let mut p = ScanDirectoryParams::new(PathBuf::from("."), u8::MAX);
        p.current_depth = 100;
        assert!(p.should_recurse());
    }
}
