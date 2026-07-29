use crate::models::cli::CliArgs;
use crate::models::video::{Rating, Tag, VideoFile, VideoFormat};
use log::warn;
use serde::Deserialize;
use std::fs;
use std::path::Path as StdPath;
use std::time::SystemTime;

pub struct ParsedVideoInfo {
    display_name: String,
    rating: Rating,
    tags: Vec<Tag>,
}
/// Helper function to parse video meta attributes (Rating, Tags) from sidecars or filename formatting
pub fn parse_video_info(path: &StdPath) -> ParsedVideoInfo {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown Video".to_string());

    if title.starts_with('[') {
        if let Some(end_idx) = title.find(']') {
            let rating_val = title[1..end_idx].trim();
            if let Ok(r_num) = rating_val.parse::<u8>() {
                rating = Rating::from_u8(r_num);
                title = title[end_idx + 1..].trim().to_string();
            }
        }
    }

    // Extract Tags
    if title.starts_with('[') {
        if let Some(end_idx) = title.find(']') {
            let tags_val = &title[1..end_idx];
            tags = tags_val.split(',').map(|t| Tag::from_str(t)).collect();
            title = title[end_idx + 1..].trim().to_string();
        }
    }

    let display_name = title.replace('_', " ").replace('.', " ");
    ParsedVideoInfo {
        display_name,
        rating,
        tags,
    }
}

pub struct ScanDirectoryParams<'a> {
    /// cli args used to infer everything
    pub cli_args: &'a CliArgs,
    /// The base directory from which the scan started, used for relative paths
    pub base_path: &'a PathBuf,
    /// depth of recursion
    pub depth: u8,
    pub count: usize,
    pub videos: Vec<VideoFile>,
}
impl<'a> ScanDirectoryParams<'a> {
    const THUMBNAIL_EXT: [&'static str; 4] = ["jpg", "jpeg", "png", "webp"];
    pub fn new(cli_args: &'a CliArgs) -> Self {
        Self {
            cli_args,
            base_path: &cli_args.path,
            depth: 0,
            count: 0,
            videos: Vec::new(),
        }
    }

    /// true if cli args max depth if 0, false otherwise
    pub fn is_recursive(&self) -> bool {
        self.cli_args.max_depth == 0
    }
    pub fn get_videos(self) -> Vec<VideoFile> {
        self.videos
    }
}

pub fn scan_dir(params: &mut ScanDirectoryParams) {
    let current_dir = &params.cli_args.path;
    let base_dir = &params.base_path;

    if let Ok(entries) = fs::read_dir(current_dir) {
        info!("Scanning directory: {}", current_dir.display());
        for entry in entries.flatten() {
            let e_path = entry.path();
            if params.is_recursive() && e_path.is_dir() {
                let mut sub_params = ScanDirectoryParams {
                    cli_args: &params.cli_args,
                    base_path: &e_path, // path remains the same
                    depth: params.depth + 1,
                    count: 0,           // Reset count for sub-call, will be accumulated
                    videos: Vec::new(), // Reset videos for sub-call, will be accumulated
                };
                sub_params.base_path = &e_path; // Update path for the sub-call
                scan_dir(&mut sub_params);

                // Accumulate results from the sub-call
                params.count += sub_params.count;
                params.videos.extend(sub_params.videos);
            } else if e_path.is_file() {
                params.count += 1;

                // Show dynamic scanner progress in console on startup
                if params.count % 10 == 0 || params.count == 1 {
                    print!(
                        "\r\x1b[2K\x1b[1;33m[INFO]\x1b[0m Filesystem indexing: {} items found...",
                        params.count
                    );

                    /*print!(
                        "\r\x1b[2K\x1b[1;33m Filesystem indexing: {} items found...",
                        params.count
                    );*/
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }

                if let Some(ext) = e_path.extension().and_then(|s| s.to_str()) {
                    if let Some(format) = VideoFormat::from_ext(ext) {
                        // Calculate web-friendly relative path from base_dir for streaming url
                        let file_name = e_path
                            .strip_prefix(base_dir)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| e_path.to_string_lossy().to_string())
                            .replace('\\', "/");

                        let video_info: ParsedVideoInfo = parse_video_info(&e_path);

                        // Scan for thumbnail sidecar (e.g. video.jpg for video.mp4) todo not working yet
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

                        if let Ok(metadata) = fs::metadata(&e_path) {
                            let file_size_mb = metadata.len() / (1024 * 1024);
                            let modified_time = metadata.modified().unwrap_or(SystemTime::now());
                            let unix_timestamp = modified_time
                                .duration_since(SystemTime::UNIX_EPOCH)
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
                        } else {
                            warn!("Failed to retrieve metadata for file: {}", e_path.display());
                        }
                    }
                }
            }
        }
    } else {
        warn!(
            "Failed to read contents of directory: {}",
            current_dir.display()
        );
    }
}

/// Recursively or non-recursively scans directory for supported video files
pub fn scan_directory(
    dir: &StdPath,
    base_dir: &StdPath,
    recurse: bool,
    videos: &mut Vec<VideoFile>,
    count: &mut usize,
) {
    } else {
        warn!("Failed to read contents of directory: {}", dir.display());
    }
}
