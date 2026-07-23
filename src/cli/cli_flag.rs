use std::path::PathBuf;

/// Strongly typed cli-line arguments
#[derive(Debug, PartialEq, Eq)]
pub enum CliFlag {
    NoRecurse,
    Port(u16),
    Path(PathBuf),
}