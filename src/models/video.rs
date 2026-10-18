/// Represents the supported video containers/formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFormat {
    /// MPEG-4 container.
    Mp4,
    /// Matroska container.
    Mkv,
    /// WebM container.
    Webm,
    /// QuickTime container.
    Mov,
    /// Audio Video Interleave container.
    Avi,
    /// M4V container.
    M4v,
}

impl VideoFormat {
    /// Supported extensions for [VideoFormat]
    pub const SUPPORTED_EXTS: &'static [&'static str] =
        &["mp4", "mkv", "webm", "mov", "avi", "m4v"];

    /// Returns the `VideoFormat` for the given file extension, or `None` if unsupported.
    pub fn from_ext(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "mp4" => Some(Self::Mp4),
            "mkv" => Some(Self::Mkv),
            "webm" => Some(Self::Webm),
            "mov" => Some(Self::Mov),
            "avi" => Some(Self::Avi),
            "m4v" => Some(Self::M4v),
            _ => None,
        }
    }

    /// Returns the canonical lowercase extension string for this format.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
            Self::Webm => "webm",
            Self::Mov => "mov",
            Self::Avi => "avi",
            Self::M4v => "m4v",
        }
    }
}

/// A video file discovered during directory scanning.
#[derive(Debug, Clone)]
pub struct VideoFile {
    /// Web-friendly relative path used as the streaming URL key.
    pub file_name: String,
    /// Human-readable title derived from the filename.
    pub display_name: String,
    /// File size in megabytes.
    pub file_size_mb: u64,
    /// Unix modification timestamp in seconds.
    pub unix_timestamp: u64,
    /// Container format of the video.
    pub format: VideoFormat,
    /// Relative path to a sidecar thumbnail image, if found.
    pub thumbnail_path: Option<String>,
}
