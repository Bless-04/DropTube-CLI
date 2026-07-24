use log::Level;

pub fn create_log(level: Level) -> Result<(), log::SetLoggerError> {
    stderrlog::new()
        .module(stringify!(droptube))
        .verbosity(level)
        .init()
}