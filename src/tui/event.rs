//! Terminal keyboard and mouse input handling.

use super::ui::Page;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use std::io;
use std::time::Duration;

/// Action requested by the latest terminal input event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InputAction {
    /// Continue rendering and accepting input.
    Continue,
    /// Display the preceding dashboard page.
    PreviousPage,
    /// Display the following dashboard page.
    NextPage,
    /// Display a particular dashboard page.
    Show(Page),
    /// Move the current page toward older or earlier entries.
    ScrollBackward,
    /// Move the current page toward newer or later entries.
    ScrollForward,
    /// Move the current page to its first entry.
    ScrollToStart,
    /// Move the current page to its latest entry.
    ScrollToEnd,
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
    let Event::Key(key_event) = event else {
        return InputAction::Continue;
    };
    if !key_event.is_press() {
        return InputAction::Continue;
    }

    match key_event.code {
        KeyCode::Char('q' | 'Q') | KeyCode::Esc => InputAction::Quit,
        KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            InputAction::Quit
        }
        KeyCode::Up | KeyCode::Left | KeyCode::BackTab => InputAction::PreviousPage,
        KeyCode::Down | KeyCode::Right | KeyCode::Tab => InputAction::NextPage,
        KeyCode::PageUp => InputAction::ScrollBackward,
        KeyCode::PageDown => InputAction::ScrollForward,
        KeyCode::Home => InputAction::ScrollToStart,
        KeyCode::End => InputAction::ScrollToEnd,
        KeyCode::Char('1' | 'd' | 'D') => InputAction::Show(Page::Dashboard),
        KeyCode::Char('2' | 'l' | 'L') => InputAction::Show(Page::Logs),
        KeyCode::Char('3' | 'r' | 'R') => InputAction::Show(Page::QrCode),
        _ => InputAction::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn quit_keys_only_match_key_presses() {
        assert_eq!(action_for(key(KeyCode::Char('q'))), InputAction::Quit);
        assert_eq!(action_for(key(KeyCode::Char('Q'))), InputAction::Quit);
        assert_eq!(action_for(key(KeyCode::Esc)), InputAction::Quit);
        assert_eq!(
            action_for(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL
            ))),
            InputAction::Quit
        );
        assert_eq!(
            action_for(Event::Key(KeyEvent::new_with_kind(
                KeyCode::Char('q'),
                KeyModifiers::NONE,
                KeyEventKind::Release
            ))),
            InputAction::Continue
        );
    }

    #[test]
    fn navigation_keys_map_to_page_actions() {
        for code in [KeyCode::Up, KeyCode::Left, KeyCode::BackTab] {
            assert_eq!(action_for(key(code)), InputAction::PreviousPage);
        }
        for code in [KeyCode::Down, KeyCode::Right, KeyCode::Tab] {
            assert_eq!(action_for(key(code)), InputAction::NextPage);
        }
        assert_eq!(
            action_for(key(KeyCode::Char('1'))),
            InputAction::Show(Page::Dashboard)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('l'))),
            InputAction::Show(Page::Logs)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('r'))),
            InputAction::Show(Page::QrCode)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('D'))),
            InputAction::Show(Page::Dashboard)
        );
    }

    #[test]
    fn scrolling_keys_map_to_scroll_actions() {
        assert_eq!(
            action_for(key(KeyCode::PageUp)),
            InputAction::ScrollBackward
        );
        assert_eq!(
            action_for(key(KeyCode::PageDown)),
            InputAction::ScrollForward
        );
        assert_eq!(action_for(key(KeyCode::Home)), InputAction::ScrollToStart);
        assert_eq!(action_for(key(KeyCode::End)), InputAction::ScrollToEnd);
    }

    #[test]
    fn mouse_input_does_not_exit_the_interface() {
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 2,
            row: 3,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(action_for(Event::Mouse(mouse_event)), InputAction::Continue);
    }
}
