//! Opt-in FFmpeg thumbnail generation. Callers serialize scans; FFmpeg runs asynchronously.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::{fs, process::Command, time::timeout};

/// A validated FFmpeg executable used to create cached JPEG thumbnails.
#[derive(Clone, Debug)]
pub struct ThumbnailGenerator {
    executable: PathBuf,
}

impl ThumbnailGenerator {
}
