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
    #[arg(short = 'r',long, visible_aliases = ["depth","recurse","recursive"], default_value_t = 0)]
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
    #[arg(long, requires = "thumbnails",value_parser=CliArgs::validate_path)]
    pub ffmpeg_path: Option<PathBuf>,

    /// Root path for files (default: current directory).
    #[arg(default_value = "./", value_parser = CliArgs::validate_dir)]
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
        let args_result = CliArgs::try_parse_from([EXECUTABLE, "--depth", "3"]);
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
        let args_result = CliArgs::try_parse_from([EXECUTABLE, ".", "-r", "5"]);
        let args = args_result.expect("valid args");
        assert_eq!(args.max_depth, 5);
    }

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(prefix: &str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "droptube-{prefix}-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("create test dir");
            Self(path)
        }

        fn create_file(&self, name: &str) -> PathBuf {
            let file_path = self.0.join(name);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).expect("create parent dirs");
            }
            std::fs::write(&file_path, b"dummy content").expect("create test file");
            file_path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn thumbnail_generation_is_explicitly_enabled() {
        let fixture = TestDir::new("cli-thumb");
        let fake_ffmpeg = fixture.create_file("fake_ffmpeg.exe");
        let ffmpeg_str = fake_ffmpeg.to_str().expect("valid utf-8 path");

        let args = CliArgs::try_parse_from([
            EXECUTABLE,
            "--use-thumbnails",
            "--ffmpeg-path",
            ffmpeg_str,
        ])
        .expect("valid thumbnail options");
        assert!(args.thumbnails);
        assert_eq!(args.ffmpeg_path, Some(fake_ffmpeg.clone()));

        // --ffmpeg-path without --thumbnails or its aliases must fail
        assert!(CliArgs::try_parse_from([EXECUTABLE, "--ffmpeg-path", ffmpeg_str]).is_err());
    }

    #[test]
    fn thumbnail_generation_aliases_work() {
        for flag in ["--thumbnails", "--use-thumbnails", "--generate-thumbnails"] {
            let args = CliArgs::try_parse_from([EXECUTABLE, flag]).expect("valid thumbnail flag");
            assert!(args.thumbnails, "expected {flag} to enable thumbnails");
        }
    }

    #[test]
    fn ffmpeg_path_fails_if_file_does_not_exist() {
        let missing_path = "non_existent_tools/ffmpeg";
        let result = CliArgs::try_parse_from([
            EXECUTABLE,
            "--use-thumbnails",
            "--ffmpeg-path",
            missing_path,
        ]);
        assert!(result.is_err(), "expected non-existent ffmpeg path to fail");
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("does not exist"),
            "expected error to mention 'does not exist', got: {err_msg}"
        );
    }

    #[test]
    fn path_validation_accepts_existing_directory() {
        let fixture = TestDir::new("cli-path-ok");
        let dir_str = fixture.0.to_str().expect("valid utf-8 path");

        let args = CliArgs::try_parse_from([EXECUTABLE, dir_str]).expect("valid directory arg");
        assert_eq!(args.path, fixture.0);
    }

    #[test]
    fn path_validation_rejects_missing_directory() {
        let missing = "totally_missing_movie_library_dir_42";
        let result = CliArgs::try_parse_from([EXECUTABLE, missing]);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("does not exist"));
    }

    #[test]
    fn path_validation_rejects_file_as_directory() {
        let fixture = TestDir::new("cli-not-a-dir");
        let file = fixture.create_file("not_a_directory.txt");
        let file_str = file.to_str().expect("valid utf-8 path");

        let result = CliArgs::try_parse_from([EXECUTABLE, file_str]);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("is not a directory"));
    }

    #[test]
    fn depth_aliases_work() {
        for flag in ["-r", "--depth", "--recurse", "--recursive"] {
            let args =
                CliArgs::try_parse_from([EXECUTABLE, flag, "4"]).expect("valid depth flag alias");
            assert_eq!(args.max_depth, 4, "flag {flag} failed to set depth");
        }
    }

    #[test]
    fn tui_aliases_work() {
        for flag in ["--open-tui", "--use-tui"] {
            let args = CliArgs::try_parse_from([EXECUTABLE, flag]).expect("valid tui flag alias");
            assert!(args.open_tui, "flag {flag} failed to set open_tui");
        }
    }

    #[test]
    fn port_parsing_accepts_valid_and_rejects_invalid() {
        let args_short = CliArgs::try_parse_from([EXECUTABLE, "-p", "9090"]).expect("valid port");
        assert_eq!(args_short.port, Some(9090));

        let args_long =
            CliArgs::try_parse_from([EXECUTABLE, "--port", "12345"]).expect("valid port");
        assert_eq!(args_long.port, Some(12345));

        // Out of u16 range
        assert!(CliArgs::try_parse_from([EXECUTABLE, "-p", "70000"]).is_err());
        // Non-numeric
        assert!(CliArgs::try_parse_from([EXECUTABLE, "-p", "invalid"]).is_err());
    }
}
