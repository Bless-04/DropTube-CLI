use log::Level;
use stderrlog::ColorChoice;

pub fn create_log(level: Level) -> Result<(), log::SetLoggerError> {
    stderrlog::new()
        .module(stringify!(droptube))
        .verbosity(level)
        .color(ColorChoice::Auto)
        .init()
}
