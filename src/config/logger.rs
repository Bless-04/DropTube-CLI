use log::Level;
use stderrlog::ColorChoice;

/// Initializes the application logger at the requested verbosity.
pub fn create_log(level: Level) -> Result<(), log::SetLoggerError> {
    stderrlog::new()
        .module(stringify!(droptube))
        .verbosity(level)
        .color(ColorChoice::Auto)
        .init()
}
