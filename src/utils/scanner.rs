pub fn parse_video_info(path: &StdPath) -> (String, Rating, Vec<Tag>) {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown Video".to_string());

}
