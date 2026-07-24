use std::path::PathBuf;

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

    pub fn is_rated(&self) -> bool {
        !matches!(self, Self::Unrated)
    }
}

/// Represents categories/topics tags for video grouping
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
    pub fn from_str(s: &str) -> Self {
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

#[derive(Debug, Clone)]
pub struct VideoFile {
    pub file_name: String,
    pub display_name: String,
    pub file_size_mb: u64,
    pub unix_timestamp: u64,
    pub format: VideoFormat,
    pub rating: Rating,
    pub tags: Vec<Tag>,
    pub thumbnail_path: Option<String>,
}

