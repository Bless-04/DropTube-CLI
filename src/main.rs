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
