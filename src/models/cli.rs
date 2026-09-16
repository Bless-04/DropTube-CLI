use clap::Parser;
use std::path::PathBuf;
use std::sync::LazyLock;

/// Clap-based argument definition used for `--help` generation and parsing.
#[derive(Parser, Debug)]
#[command(author="Blessing", version, about, long_about = None)]
pub struct CliArgs {
    /// Maximum subfolder depth to recurse into.
    ///
    /// `0` = current directory only, `255` = unlimited (default: `0`).
    #[arg(short = 'd',long, visible_aliases = ["depth"], default_value_t = 0)]
    pub max_depth: u8,

    /// Explicit port to listen on. If not provided, defaults to 8081 and scans upward.
    #[arg(short = 'p', long)]
    pub port: Option<u16>,

    /// Open the interactive terminal interface while the server runs.
    #[arg(long, visible_aliases = ["use-tui"])]
    pub open_tui: bool,

    /// Generate missing thumbnails with FFmpeg. Will fail at startup if FFmpeg is unavailable.
    #[arg(long,visible_aliases = ["use-thumbnails","generate-thumbnails"])]
    pub thumbnails: bool,

    /// FFmpeg executable to use (otherwise resolved from PATH). Requires --thumbnails.
    #[arg(long, requires = "thumbnails")]
    pub ffmpeg_path: Option<PathBuf>,

    /// Root path for files (default: current directory).
    #[arg(long,default_value = "./", value_parser = CliArgs::validate_dir)]
    pub path: PathBuf,
}

/// Startup Validation
impl CliArgs {
    fn validate_path(path_str: &str) -> Result<PathBuf, String> {
        let path = PathBuf::from(path_str);
        match path.exists() {
            true => Ok(path),
            false => Err(format!("Path {} does not exist", path_str)),
        }
    }
    /// Validates the directory passed by args
    fn validate_dir(path_str: &str) -> Result<PathBuf, String> {
        let path = Self::validate_path(path_str)?;
        if !path.is_dir() {
            return Err(format!(
                "The path '{path_str}' exists, but it is not a directory."
            ));
        }
        Ok(path)
    }
}

/// Global singleton for the parsed CLI args, initialised lazily from `std::env::args`.
static CLI_ARGS: LazyLock<CliArgs> = LazyLock::new(CliArgs::parse);

/// Returns the global [`CliArgs`] singleton.
pub fn get() -> &'static CliArgs {
    &CLI_ARGS
}

#[cfg(test)]
mod tests {
    /// Name of the executable used in test argument lists.
    const EXECUTABLE: &str = "droptube";
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn parses_defaults() {
        let args_result = CliArgs::try_parse_from([EXECUTABLE, "."]);
        assert!(
            args_result.is_ok(),
            "expected Ok but got: {:?}",
            args_result.err()
        );
        let args = args_result.expect("checked above");

        assert_eq!(args.port, None);
        assert_eq!(args.path, PathBuf::from("."));
        assert_eq!(args.max_depth, 0);
        assert!(!args.open_tui);
        assert!(!args.thumbnails);
        assert!(args.ffmpeg_path.is_none());
    }

    #[test]
    fn tui_is_only_enabled_by_its_flag() {
        let args_result = CliArgs::try_parse_from([EXECUTABLE, "--open-tui"]);
        assert!(args_result.is_ok());
        if let Ok(args) = args_result {
            assert!(args.open_tui);
        }

        let default_result = CliArgs::try_parse_from([EXECUTABLE]);
        assert!(default_result.is_ok());
        if let Ok(args) = default_result {
            assert!(!args.open_tui);
        }
    }

    #[test]
    fn parse_args_depth_flag() {
        let args_result = CliArgs::try_parse_from([EXECUTABLE, "--depth", "3", "."]);
        let args = args_result.expect("valid args");
        assert_eq!(args.max_depth, 3);
        assert_eq!(args.path, PathBuf::from("."));
    }

    #[test]
    fn parse_args_rejects_invalid_depth() {
        let args_result = CliArgs::try_parse_from([EXECUTABLE, "--depth", "999", "."]);
        assert!(args_result.is_err());
    }

    #[test]
    fn parse_args_depth_short_flag() {
        let args_result = CliArgs::try_parse_from([EXECUTABLE, "-d", "5", "."]);
        let args = args_result.expect("valid args");
        assert_eq!(args.max_depth, 5);
    }

    #[test]
    fn thumbnail_generation_is_explicitly_enabled() {
        let args = CliArgs::try_parse_from([
            EXECUTABLE,
            "--use-thumbnails",
            "--ffmpeg-path",
            "tools/ffmpeg",
        ])
        .expect("valid thumbnail options");
        assert!(args.thumbnails);
        assert_eq!(args.ffmpeg_path, Some(PathBuf::from("tools/ffmpeg")));
        assert!(CliArgs::try_parse_from([EXECUTABLE, "--ffmpeg-path", "ffmpeg"]).is_err());
    }
}
