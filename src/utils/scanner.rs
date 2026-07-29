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
/// Recursively or non-recursively scans directory for supported video files
pub fn scan_directory(
    dir: &StdPath,
    base_dir: &StdPath,
    recurse: bool,
    videos: &mut Vec<VideoFile>,
    count: &mut usize,
) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if recurse && path.is_dir() {
                scan_directory(&path, base_dir, recurse, videos, count);
            } else if path.is_file() {
                *count += 1;

                // Show dynamic scanner progress in console on startup
                if *count % 10 == 0 || *count == 1 {
                    print!(
                        "\r\x1b[2K\x1b[1;33m[INFO]\x1b[0m Filesystem indexing: {} items found...",
                        count
                    );
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }

                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if let Some(format) = VideoFormat::from_ext(ext) {
                        // Calculate web-friendly relative path from base_dir for streaming url
                        let file_name = path
                            .strip_prefix(base_dir)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| path.to_string_lossy().to_string())
                            .replace('\\', "/");

                        let video_info: ParsedVideoInfo = parse_video_info(&path);

                        // Scan for thumbnail sidecar (e.g. video.jpg for video.mp4) todo not working yet
                        let mut thumbnail_path = None;
                        for img_ext in &["jpg", "jpeg", "png", "webp"] {
                            let mut test_thumb = path.clone();
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

                        if let Ok(metadata) = fs::metadata(&path) {
                            let file_size_mb = metadata.len() / (1024 * 1024);
                            let modified_time = metadata.modified().unwrap_or(SystemTime::now());
                            let unix_timestamp = modified_time
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0);

                            videos.push(VideoFile {
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
                            warn!("Failed to retrieve metadata for file: {}", path.display());
                        }
                    }
                }
            }
        }
    } else {
        warn!("Failed to read contents of directory: {}", dir.display());
    }
}
