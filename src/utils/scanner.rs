pub fn parse_video_info(path: &StdPath) -> (String, Rating, Vec<Tag>) {
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
    } else {
        warn!(
            "Failed to read contents of directory: {}",
            dir.display()
        );
    }
}
