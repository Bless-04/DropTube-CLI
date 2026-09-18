//! Terminal initialization and restoration without installing a process-wide panic hook.

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, stdout};

pub(super) fn try_init_terminal() -> io::Result<ratatui::DefaultTerminal> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout()))
}

pub(super) fn try_restore_terminal() -> io::Result<()> {
    let raw_mode_result = disable_raw_mode();
    let alternate_screen_result = execute!(stdout(), LeaveAlternateScreen);
    match (raw_mode_result, alternate_screen_result) {
        (Err(raw_mode_error), Err(screen_error)) => Err(io::Error::other(format!(
            "failed to disable raw mode: {raw_mode_error}; failed to leave alternate screen: {screen_error}"
        ))),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}
