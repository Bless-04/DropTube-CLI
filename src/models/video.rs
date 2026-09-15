/// Represents the supported video containers/formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFormat {
    Mp4,
    Mkv,
    Webm,
    Mov,
    Avi,
    M4v,
}

impl VideoFormat {
    /// Supported extensions for [VideoFormat]
    pub const SUPPORTED_EXTS: &'static [&'static str] = &["mp4", "mkv", "webm","mov","avi","m4v"];
    
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

/// Represents the rating of a video (Unrated or 1-5 stars)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rating {
    Unrated,
    OneStar,
    TwoStars,
    ThreeStars,
    FourStars,
    FiveStars,
}

impl Rating {
    /// Converts a `u8` in the range 1–5 to the corresponding `Rating` variant.
    /// Any value outside that range maps to `Rating::Unrated`.
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::OneStar,
            2 => Self::TwoStars,
            3 => Self::ThreeStars,
            4 => Self::FourStars,
            5 => Self::FiveStars,
            _ => Self::Unrated,
        }
    }

    /// Returns a star-glyph string representing this rating.
    pub fn as_stars(&self) -> &'static str {
        match self {
            Self::Unrated => "☆☆☆☆☆",
            Self::OneStar => "★☆☆☆☆",
            Self::TwoStars => "★★☆☆☆",
            Self::ThreeStars => "★★★☆☆",
            Self::FourStars => "★★★★☆",
            Self::FiveStars => "★★★★★",
        }
    }

    /// Returns `true` if this is any rating other than `Unrated`.
    pub fn is_rated(&self) -> bool {
        !matches!(self, Self::Unrated)
    }
}

/// Represents categories/topic tags for video grouping
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tag {
    Action,
    Comedy,
    Drama,
    SciFi,
    Documentary,
    Technology,
    Rust,
    Other(String),
}

impl Tag {
    /// Parses a tag from a string slice, case-insensitively.
    ///
    /// This is an infallible conversion; unrecognised strings become [`Tag::Other`].
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().trim() {
            "action" => Self::Action,
            "comedy" => Self::Comedy,
            "drama" => Self::Drama,
            "sci-fi" | "scifi" => Self::SciFi,
            "documentary" => Self::Documentary,
            "technology" | "tech" => Self::Technology,
            "rust" => Self::Rust,
            other => Self::Other(other.to_string()),
        }
    }

    /// Returns the canonical display string for this tag.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Action => "Action",
            Self::Comedy => "Comedy",
            Self::Drama => "Drama",
            Self::SciFi => "Sci-Fi",
            Self::Documentary => "Documentary",
            Self::Technology => "Technology",
            Self::Rust => "Rust",
            Self::Other(s) => s,
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
    /// Star rating parsed from the filename prefix.
    pub rating: Rating,
    /// Topic tags parsed from the filename.
    pub tags: Vec<Tag>,
    /// Relative path to a sidecar thumbnail image, if found.
    pub thumbnail_path: Option<String>,
}
