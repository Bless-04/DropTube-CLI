use clap::Parser;
use log::error;
use std::path::PathBuf;
use std::sync::{LazyLock, OnceLock};

/// Strongly typed config-line arguments
#[derive(Debug, PartialEq, Eq)]
pub enum CliFlag {
    NoRecurse,
    Port(u16),
    Path(PathBuf),
}

impl CliFlag {
    /// Parse arguments into strongly typed CliFlags
    pub fn parse_args(args: &[String]) -> Result<Vec<CliFlag>, String> {
        let mut flags = Vec::new();
        let mut iter = args.iter().skip(1);

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--no-recurse" => {
                    flags.push(CliFlag::NoRecurse);
                }
                "--port" | "-p" => {
                    if let Some(port_str) = iter.next() {
                        if let Ok(port) = port_str.parse::<u16>() {
                            flags.push(CliFlag::Port(port));
                        } else {
                            return Err(format!("Invalid port number: {}", port_str));
                        }
                    } else {
                        return Err("Missing port value after --port flag".to_string());
                    }
                }
                other => {
                    if other.starts_with('-') {
                        return Err(format!("Unknown flag: {}", other));
                    }
                    flags.push(CliFlag::Path(PathBuf::from(other)));
                }
            }
        }
        Ok(flags)
    }
}

#[derive(Parser, Debug)]
#[command(arg_required_else_help = true)]
pub struct CliArgs {
    /// Set to true if you want to look through subdirectories as well as the current directory
    #[arg(short = 'r', long = "recurse", default_value_t = 0)]
    pub max_depth: u8,

    /// port to listen on
    #[arg(default_value_t = 8081)]
    pub port: u16,

    /// root path for files
    #[arg(default_value = "./")]
    pub path: PathBuf,
}

// initializes itself from std::env::args if nothing has
static CLI_ARGS: LazyLock<CliArgs> = LazyLock::new(|| match CliArgs::try_parse() {
    Ok(parsed_args) => {
        return parsed_args;
    }
    Err(e) => {
        error!("CLI Error: {}", e);
        CliArgs::parse()
    }
});

pub fn get() -> &'static CliArgs {
    &CLI_ARGS
}
#[cfg(test)]
mod tests {
    /// Name of the executable
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
        debug_assert!(args_result.is_ok()); //todo fix this failing
        let args = args_result.unwrap();

        assert_eq!(args.port, 8081);
        assert_eq!(args.path, PathBuf::from("./test"));
        assert_eq!(args.max_depth, 0);
    }
}
