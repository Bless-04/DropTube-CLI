use log::{Level, Log, Metadata, Record};
use std::sync::{Arc, Mutex};
use stderrlog::ColorChoice;

const TUI_LOG_CAPACITY: usize = 500;

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

    #[cfg(test)]
    pub(crate) fn screen_active(&self) -> bool {
        *self
            .screen_active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

struct TuiLoggerBridge {
    level: Level,
    control: TuiLogControl,
    tui: tui_logger::Drain,
    stderr: stderrlog::StdErrLog,
}

impl TuiLoggerBridge {
    fn new(level: Level, control: TuiLogControl) -> Self {
        let mut stderr = stderrlog::new();
        stderr
            .module(stringify!(droptube))
            .verbosity(level)
            .color(ColorChoice::Auto);
        Self {
            level,
            control,
            tui: tui_logger::Drain::new(),
            stderr,
        }
    }
}

impl Log for TuiLoggerBridge {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level && is_droptube_target(metadata.target())
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        self.tui.log(record);
        // Hold this lock across stderrlog's write so entering the alternate screen cannot race a
        // record that already decided to write to stderr.
        let screen_active = self
            .control
            .screen_active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !*screen_active {
            self.stderr.log(record);
        }
    }

    fn flush(&self) {
        self.stderr.flush();
    }
}

fn is_droptube_target(target: &str) -> bool {
    target == stringify!(droptube)
        || target
            .strip_prefix(stringify!(droptube))
            .is_some_and(|suffix| suffix.starts_with("::"))
}

/// Initializes `tui-logger` storage for the opt-in terminal interface.
///
/// The bridge feeds DropTube records into the crate's bounded history and preserves stderr output
/// before and after the alternate screen is active. The headless logger remains independent.
pub fn create_tui_log(level: Level) -> Result<TuiLogControl, log::SetLoggerError> {
    tui_logger::set_buffer_depth(TUI_LOG_CAPACITY);
    let control = TuiLogControl::new();
    let logger = TuiLoggerBridge::new(level, control.clone());
    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(level.to_level_filter());
    Ok(control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tui_bridge_filters_unrelated_targets_and_excess_verbosity() {
        let bridge = TuiLoggerBridge::new(Level::Info, TuiLogControl::new());
        let app_info = Metadata::builder()
            .level(Level::Info)
            .target("droptube::server")
            .build();
        let app_debug = Metadata::builder()
            .level(Level::Debug)
            .target("droptube::server")
            .build();
        let dependency_error = Metadata::builder()
            .level(Level::Error)
            .target("dependency")
            .build();

        assert!(bridge.enabled(&app_info));
        assert!(!bridge.enabled(&app_debug));
        assert!(!bridge.enabled(&dependency_error));
    }

    #[test]
    fn tui_log_control_tracks_the_alternate_screen() {
        let control = TuiLogControl::new();

        control.set_screen_active(true);
        assert!(control.screen_active());

        control.set_screen_active(false);
        assert!(!control.screen_active());
    }
}
