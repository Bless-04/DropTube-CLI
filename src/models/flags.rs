use std::path::PathBuf;

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
