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
    #[arg(short = 'd', long = "depth", default_value_t = 0)]
    pub max_depth: u8,

    /// Explicit port to listen on. If not provided, defaults to 8081 and scans upward.
    #[arg(short = 'p', long = "port")]
    pub port: Option<u16>,

    /// Root path for files (default: current directory).
    #[arg(default_value = "./")]
    pub path: PathBuf,
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
        let args_result = CliArgs::try_parse_from([EXECUTABLE, "./test"]);
        assert!(
            args_result.is_ok(),
            "expected Ok but got: {:?}",
            args_result.err()
        );
        let args = args_result.expect("checked above");

        assert_eq!(args.port, None);
        assert_eq!(args.path, PathBuf::from("./test"));
        assert_eq!(args.max_depth, 0);
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
}
