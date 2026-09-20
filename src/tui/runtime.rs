//! Terminal dashboard lifecycle and event-loop orchestration.

use super::event::{self, InputAction};
use super::startup::{try_init_terminal, try_restore_terminal};
use super::ui::{self, Page, TuiConfig};
use crate::config::logger::TuiLogControl;
use crate::server::{ClientSnapshot, ServerState};
use log::LevelFilter;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tui_logger::{TuiWidgetEvent, TuiWidgetState};

const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const SCROLL_STEP: usize = 5;

#[derive(Debug, Default)]
pub(super) struct ViewState {
    pub(super) page: Page,
    pub(super) client_scroll: usize,
}

struct TuiScreenGuard {
    log_control: TuiLogControl,
}

struct TerminalRestoreGuard {
    armed: bool,
    log_control: TuiLogControl,
}

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        if self.armed
            && let Err(error) = try_restore_terminal()
        {
            self.log_control.set_screen_active(false);
            log::error!("Failed to restore terminal after interface panic: {error}");
        }
    }
}

impl Drop for TuiScreenGuard {
    fn drop(&mut self) {
        self.log_control.set_screen_active(false);
    }
}

/// Runs the terminal dashboard with a bounded application-log history.
///
/// This is the entry point used by `--open-tui`; log records are rendered by Ratatui instead of
/// being written directly over the alternate screen. Pressing `q`, Escape, or Ctrl+C returns
/// control to the caller, which can then gracefully stop the server task.
pub fn run_with_logs(
    state: Arc<Mutex<ServerState>>,
    config: TuiConfig,
    server_running: Arc<AtomicBool>,
    log_control: TuiLogControl,
) -> io::Result<()> {
    log_control.set_screen_active(true);
    let _screen_guard = TuiScreenGuard {
        log_control: log_control.clone(),
    };
    let mut terminal = match try_init_terminal() {
        Ok(terminal) => terminal,
        Err(error) => {
            if let Err(restore_error) = try_restore_terminal() {
                return Err(io::Error::other(format!(
                    "{error}; terminal restoration also failed: {restore_error}"
                )));
            }
            return Err(error);
        }
    };
    let mut restore_guard = TerminalRestoreGuard {
        armed: true,
        log_control: log_control.clone(),
    };

    let run_result = run_event_loop(&mut terminal, &state, &config, &server_running);
    let restore_result = try_restore_terminal();
    log_control.set_screen_active(false);
    if restore_result.is_ok() {
        restore_guard.armed = false;
    }
    match (run_result, restore_result) {
        (Err(error), Err(restore_error)) => Err(io::Error::other(format!(
            "{error}; terminal restoration also failed: {restore_error}"
        ))),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn run_event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    state: &Arc<Mutex<ServerState>>,
    config: &TuiConfig,
    server_running: &AtomicBool,
) -> io::Result<()> {
    let mut view_state = ViewState::default();
    let log_state = TuiWidgetState::new().set_default_display_level(LevelFilter::Info);
    while server_running.load(Ordering::Acquire) {
        let clients = snapshot_clients(state);
        tui_logger::move_events();
        terminal.draw(|frame| {
            ui::render(frame, config, &clients, &view_state, &log_state);
        })?;

        match event::poll_action(EVENT_POLL_INTERVAL)? {
            InputAction::Continue => {}
            InputAction::PreviousPage => view_state.page = view_state.page.previous(),
            InputAction::NextPage => view_state.page = view_state.page.next(),
            InputAction::Show(selected_page) => view_state.page = selected_page,
            InputAction::ScrollBackward => match view_state.page {
                Page::Dashboard => {
                    view_state.client_scroll = view_state.client_scroll.saturating_sub(SCROLL_STEP);
                }
                Page::Logs => {
                    log_state.transition(TuiWidgetEvent::PrevPageKey);
                }
                Page::QrCode => {}
            },
            InputAction::ScrollForward => match view_state.page {
                Page::Dashboard => {
                    view_state.client_scroll = view_state.client_scroll.saturating_add(SCROLL_STEP);
                }
                Page::Logs => {
                    log_state.transition(TuiWidgetEvent::NextPageKey);
                }
                Page::QrCode => {}
            },
            InputAction::ScrollToStart => match view_state.page {
                Page::Dashboard => view_state.client_scroll = 0,
                Page::Logs => log_state.transition(TuiWidgetEvent::PrevPageKey),
                Page::QrCode => {}
            },
            InputAction::ScrollToEnd => match view_state.page {
                Page::Dashboard => view_state.client_scroll = clients.len(),
                Page::Logs => log_state.transition(TuiWidgetEvent::EscapeKey),
                Page::QrCode => {}
            },
            InputAction::Quit => server_running.store(false, Ordering::Release),
        }
    }
    Ok(())
}

fn snapshot_clients(state: &Arc<Mutex<ServerState>>) -> Vec<ClientSnapshot> {
    let server_state = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    server_state.clients()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_guard_reenables_terminal_log_output_when_dropped() {
        let log_control = TuiLogControl::new();
        log_control.set_screen_active(true);
        let guard = TuiScreenGuard {
            log_control: log_control.clone(),
        };

        drop(guard);

        assert!(!log_control.screen_active());
    }
}
