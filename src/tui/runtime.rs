//! Terminal dashboard lifecycle and event-loop orchestration.

use super::event::{self, InputAction};
use super::profiles::{ProfileMonitor, ProfileTuiContext, ProfilesFeed, ProfilesStatus};
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
    pub(super) profile_scroll: usize,
    pub(super) profiles: ProfilesStatus,
}

struct TerminalContext {
    state: Arc<Mutex<ServerState>>,
    config: TuiConfig,
    server_running: Arc<AtomicBool>,
    log_control: TuiLogControl,
    profiles: Option<ProfilesFeed>,
}

struct EventLoopContext<'a> {
    state: &'a Arc<Mutex<ServerState>>,
    config: &'a TuiConfig,
    server_running: &'a AtomicBool,
    profiles: Option<&'a ProfilesFeed>,
}

struct RunningGuard(Arc<AtomicBool>);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
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
/// Log records are rendered by Ratatui instead of being written directly over the alternate
/// screen. Pressing `q`, Escape, or Ctrl+C returns control to the caller, which can then gracefully
/// stop the server task. This synchronous entry point has no profile source; use
/// [`run_with_profiles`] to attach the read-only Profiles page.
pub fn run_with_logs(
    state: Arc<Mutex<ServerState>>,
    config: TuiConfig,
    server_running: Arc<AtomicBool>,
    log_control: TuiLogControl,
) -> io::Result<()> {
    run_terminal(TerminalContext {
        state,
        config,
        server_running,
        log_control,
        profiles: None,
    })
}

/// Runs the terminal with a read-only overview of committed library profiles.
///
/// One background task refreshes the overview every five seconds and on page entry.
/// File reads never run on the terminal's render/input loop. The reader is stopped
/// and joined when the terminal exits. Cancelling this future requests terminal
/// shutdown and aborts the read-only worker; terminal restoration follows its next
/// input poll. Existing callers can continue using [`run_with_logs`].
///
/// ```no_run
/// # use std::sync::{Arc, Mutex};
/// # use std::sync::atomic::AtomicBool;
/// # use droptube::{models::state::AppState, config::logger::TuiLogControl};
/// # use droptube::server::ServerState;
/// # use droptube::tui::{self, TuiConfig, ProfileTuiContext};
/// async fn dashboard(app: AppState, config: TuiConfig, logs: TuiLogControl) -> std::io::Result<()> {
///     tui::run_with_profiles(
///         Arc::new(Mutex::new(ServerState::default())),
///         config,
///         Arc::new(AtomicBool::new(true)),
///         ProfileTuiContext::new(app, logs),
///     ).await
/// }
/// ```
pub async fn run_with_profiles(
    state: Arc<Mutex<ServerState>>,
    config: TuiConfig,
    server_running: Arc<AtomicBool>,
    context: ProfileTuiContext,
) -> io::Result<()> {
    let _running_guard = RunningGuard(Arc::clone(&server_running));
    let mut monitor = ProfileMonitor::start(context.state);
    let terminal_context = TerminalContext {
        state,
        config,
        server_running,
        log_control: context.log_control,
        profiles: Some(monitor.feed.clone()),
    };
    let terminal_result = tokio::task::spawn_blocking(move || run_terminal(terminal_context))
        .await
        .map_err(|error| io::Error::other(format!("terminal interface task failed: {error}")))
        .and_then(|result| result);
    let monitor_result = monitor.stop().await;
    match (terminal_result, monitor_result) {
        (Err(error), Err(monitor_error)) => {
            Err(io::Error::other(format!("{error}; {monitor_error}")))
        }
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn run_terminal(context: TerminalContext) -> io::Result<()> {
    let TerminalContext {
        state,
        config,
        server_running,
        log_control,
        profiles,
    } = context;
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

    let run_result = run_event_loop(
        &mut terminal,
        EventLoopContext {
            state: &state,
            config: &config,
            server_running: &server_running,
            profiles: profiles.as_ref(),
        },
    );
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
    context: EventLoopContext<'_>,
) -> io::Result<()> {
    let mut view_state = ViewState::default();
    let log_state = TuiWidgetState::new().set_default_display_level(LevelFilter::Info);
    while context.server_running.load(Ordering::Acquire) {
        let clients = snapshot_clients(context.state);
        if let Some(profiles) = context.profiles {
            view_state.profiles = profiles.snapshot();
        }
        tui_logger::move_events();
        terminal.draw(|frame| {
            ui::render(frame, context.config, &clients, &view_state, &log_state);
        })?;

        let action = event::poll_action(EVENT_POLL_INTERVAL)?;
        if action == InputAction::Quit {
            context.server_running.store(false, Ordering::Release);
        }
        if apply_action(&mut view_state, action, clients.len(), &log_state)
            && let Some(profiles) = context.profiles
        {
            profiles.request_refresh();
        }
    }
    Ok(())
}

fn apply_action(
    view_state: &mut ViewState,
    action: InputAction,
    client_count: usize,
    log_state: &TuiWidgetState,
) -> bool {
    let previous_page = view_state.page;
    match action {
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
            Page::Profiles => {
                view_state.profile_scroll = view_state.profile_scroll.saturating_sub(SCROLL_STEP);
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
            Page::Profiles => {
                view_state.profile_scroll = view_state.profile_scroll.saturating_add(SCROLL_STEP);
            }
            Page::QrCode => {}
        },
        InputAction::ScrollToStart => match view_state.page {
            Page::Dashboard => view_state.client_scroll = 0,
            Page::Logs => log_state.transition(TuiWidgetEvent::PrevPageKey),
            Page::Profiles => view_state.profile_scroll = 0,
            Page::QrCode => {}
        },
        InputAction::ScrollToEnd => match view_state.page {
            Page::Dashboard => view_state.client_scroll = client_count,
            Page::Logs => log_state.transition(TuiWidgetEvent::EscapeKey),
            Page::Profiles => {
                view_state.profile_scroll = match &view_state.profiles {
                    ProfilesStatus::Ready(profiles) => profiles.len(),
                    _ => 0,
                }
            }
            Page::QrCode => {}
        },
        InputAction::Quit => {}
    }
    view_state.page == Page::Profiles && previous_page != Page::Profiles
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
    fn profile_navigation_requests_refresh_only_when_entering_and_scrolls_independently() {
        let log_state = TuiWidgetState::new();
        let mut view = ViewState::default();
        assert!(apply_action(
            &mut view,
            InputAction::Show(Page::Profiles),
            8,
            &log_state
        ));
        assert!(!apply_action(
            &mut view,
            InputAction::Show(Page::Profiles),
            8,
            &log_state
        ));
        assert!(!apply_action(
            &mut view,
            InputAction::ScrollForward,
            8,
            &log_state
        ));
        assert_eq!(view.profile_scroll, SCROLL_STEP);
        assert_eq!(view.client_scroll, 0);
        apply_action(&mut view, InputAction::ScrollBackward, 8, &log_state);
        apply_action(&mut view, InputAction::ScrollBackward, 8, &log_state);
        assert_eq!(view.profile_scroll, 0);
        view.profile_scroll = usize::MAX;
        apply_action(&mut view, InputAction::ScrollForward, 8, &log_state);
        assert_eq!(view.profile_scroll, usize::MAX);
        apply_action(&mut view, InputAction::ScrollToStart, 8, &log_state);
        assert_eq!(view.profile_scroll, 0);
        view.profiles = ProfilesStatus::Ready(Arc::new(vec![
                crate::profiles::ProfileSummary::default();
                3
            ]));
        apply_action(&mut view, InputAction::ScrollToEnd, 8, &log_state);
        assert_eq!(view.profile_scroll, 3);
        apply_action(&mut view, InputAction::Show(Page::Dashboard), 8, &log_state);
        apply_action(&mut view, InputAction::ScrollToEnd, 8, &log_state);
        assert_eq!(view.client_scroll, 8);
        assert_eq!(view.profile_scroll, 3);
    }

    #[test]
    fn terminal_owner_requests_shutdown_on_completion_or_cancellation() {
        let running = Arc::new(AtomicBool::new(true));
        let guard = RunningGuard(Arc::clone(&running));
        drop(guard);
        assert!(!running.load(Ordering::Acquire));
    }

    #[test]
    fn screen_guard_reenables_terminal_log_output_when_dropped() {
        let log_control = TuiLogControl::new();
        log_control.set_screen_active(true);
        let guard = TuiScreenGuard {
            log_control: log_control.clone(),
        };

        drop(guard);

        assert!(
            !log_control.screen_active(),
            "TuiScreenGuard must deactivate alternate screen flag when dropped to re-enable standard terminal logs"
        );
    }
}
