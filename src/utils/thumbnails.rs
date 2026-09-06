/// A validated FFmpeg executable used to create cached JPEG thumbnails.
#[derive(Clone, Debug)]
pub struct ThumbnailGenerator {
    executable: PathBuf,
}
