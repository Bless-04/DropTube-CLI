use axum::{
    extract::{Query, State, Path},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Router,
    http::StatusCode,
};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Deserialize;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;
use tower_http::catch_panic::CatchPanicLayer;
use local_ip_address::local_ip;

// Embed the HTML & JS template at compile time
const INDEX_HTML_TEMPLATE: &str = include_str!("index.html");

/// Represents the supported video containers/formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Represents the rating of a video (Unrated or 1-5 stars)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

/// Represents categories/topics tags for video grouping
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Tag {
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
    fn from_str(s: &str) -> Self {
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

    fn as_str(&self) -> &str {
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

/// Strongly typed command-line arguments
#[derive(Debug, PartialEq, Eq)]
enum CliFlag {
    NoRecurse,
    Port(u16),
    Path(PathBuf),
}

#[derive(Debug, Clone)]
struct VideoFile {
    file_name: String,
    display_name: String,
    file_size_mb: u64,
    unix_timestamp: u64,
    format: VideoFormat,
    rating: Rating,
    tags: Vec<Tag>,
    thumbnail_path: Option<String>,
}

#[derive(Deserialize)]
struct HomeQuery {
    v: Option<String>,
}

#[derive(Clone)]
struct AppState {
    movie_directory: PathBuf,
    port: u16,
    recurse: bool,
}
}
    // Validate directory
    if !movie_directory.exists() {
        eprintln!("\x1b[1;31mError:\x1b[0m Directory '{}' does not exist.", movie_directory.display());
        std::process::exit(1);
    }
    if !movie_directory.is_dir() {
        eprintln!("\x1b[1;31mError:\x1b[0m '{}' is not a directory.", movie_directory.display());
        std::process::exit(1);
    }
    println!("\x1b[1;36m============================================================\x1b[0m");
    println!("🎬 \x1b[1;32mDropTube\x1b[0m - High-Performance Rust Media Server");
    println!("\x1b[1;36m============================================================\x1b[0m");
    println!("📂 Serving Directory : \x1b[1;34m{}\x1b[0m", canonical_dir.display());
    println!("⚙️  Scanning Mode     : \x1b[1;33m{}\x1b[0m", if recurse { "Recursive" } else { "Immediate Directory Only" });
    println!("🚀 Local Access      : \x1b[1;35mhttp://localhost:{}\x1b[0m", port);
    println!("📱 Mobile Stream LAN : \x1b[1;35mhttp://{}:{}\x1b[0m", local_ip_addr, port);
    println!("\x1b[1;36m============================================================\x1b[0m");
}
