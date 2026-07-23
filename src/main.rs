fn main() {
    println!("Hello, world!");
enum VideoFormat {
    Mp4,
    Mkv,
    Webm,
    Mov,
    Avi,
    M4v,
}

impl VideoFormat {
    fn from_ext(ext: &str) -> Option<Self> {
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

    fn as_str(&self) -> &'static str {
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

enum Rating {
    Unrated,
    OneStar,
    TwoStars,
    ThreeStars,
    FourStars,
    FiveStars,
}

impl Rating {
    fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::OneStar,
            2 => Self::TwoStars,
            3 => Self::ThreeStars,
            4 => Self::FourStars,
            5 => Self::FiveStars,
            _ => Self::Unrated,
        }
    }

    fn as_stars(&self) -> &'static str {
        match self {
            Self::Unrated => "★☆☆☆☆",
            Self::OneStar => "★☆☆☆☆",
            Self::TwoStars => "★★☆☆☆",
            Self::ThreeStars => "★★★☆☆",
            Self::FourStars => "★★★★☆",
            Self::FiveStars => "★★★★★",
        }
    }

    fn is_rated(&self) -> bool {
        !matches!(self, Self::Unrated)
    }
}

    // Validate directory
    if !movie_directory.exists() {
        eprintln!("\x1b[1;31mError:\x1b[0m Directory '{}' does not exist.", raw_dir);
        std::process::exit(1);
    }
    if !movie_directory.is_dir() {
        eprintln!("\x1b[1;31mError:\x1b[0m '{}' is not a directory.", raw_dir);
        std::process::exit(1);
    }
}
