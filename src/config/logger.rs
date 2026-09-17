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

/// Coordinates whether opt-in TUI logs may also be written to stderr.
///
/// `tui-logger` owns the log history. This handle only prevents stderr writes while Ratatui owns
/// the alternate screen, then restores ordinary error visibility during startup and shutdown.
#[derive(Clone, Debug)]
pub struct TuiLogControl {
    screen_active: Arc<Mutex<bool>>,
}
impl TuiLogControl {
    pub(crate) fn new() -> Self {
        Self {
            screen_active: Arc::new(Mutex::new(false)),
        }
    }

    pub(crate) fn set_screen_active(&self, is_active: bool) {
        let mut screen_active = self
            .screen_active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *screen_active = is_active;
    }

}
