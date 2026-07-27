use clap::Parser;
use std::path::PathBuf;

//todo use clap for this
/// Strongly typed config-line arguments
#[derive(Debug, PartialEq, Eq)]
pub enum CliFlag {
    NoRecurse,
    Port(u16),
    Path(PathBuf),
}

#[derive(Parser, Debug)]
struct CliArgs {
    #[arg(short, default_value_t = false)]
    pub recursive: bool,

    /// port to listen on
    #[arg(default_value_t = 8081)]
    pub port: u16,

    #[arg(default_value = "./")]
    pub path: PathBuf,
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

#[cfg(test)]
mod tests {
    const EXE_NAME: &str = "droptube"; 
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn parses_defaults() {
        let args_result = CliArgs::try_parse_from([EXE_NAME, "./test"]);
        assert!(args_result.is_ok());
        let args = args_result.unwrap();
        
        assert_eq!(args.port, 8080);
        assert_eq!(args.path, PathBuf::from("./test"));
        assert_eq!(args.recursive, false);
    }
}
