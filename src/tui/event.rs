//! Terminal keyboard and mouse input handling.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::io;
use std::time::Duration;

/// Action requested by the latest terminal input event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InputAction {
    /// Continue rendering and accepting input.
    Continue,
    /// Exit the terminal interface.
    Quit,
}

/// Polls for one keyboard, mouse, resize, focus, or paste event.
pub(super) fn poll_action(timeout: Duration) -> io::Result<InputAction> {
    if !event::poll(timeout)? {
        return Ok(InputAction::Continue);
    }
    Ok(action_for(event::read()?))
}

fn action_for(event: Event) -> InputAction {
    match event {
        Event::Key(key_event) if is_quit_key(key_event) => InputAction::Quit,
        Event::Key(_)
        | Event::Mouse(_)
        | Event::Paste(_)
        | Event::Resize(_, _)
        | Event::FocusGained
        | Event::FocusLost => InputAction::Continue,
    }
}

fn is_quit_key(key_event: KeyEvent) -> bool {
    key_event.is_press()
        && (matches!(key_event.code, KeyCode::Char('q') | KeyCode::Esc)
            || (key_event.code == KeyCode::Char('c')
                && key_event.modifiers.contains(KeyModifiers::CONTROL)))
}

