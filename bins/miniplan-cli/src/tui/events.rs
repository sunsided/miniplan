//! Translation of raw crossterm terminal events into the TUI's own [`AppEvent`]s.

use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};

/// An input event as the TUI understands it, independent of the terminal backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    /// Ctrl+C: leave the TUI immediately.
    Quit,
    /// Backspace.
    Back,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Tab: move focus forward.
    Tab,
    /// Shift+Tab: move focus backward.
    BackTab,
    /// A character key; modifiers other than Ctrl+C are ignored, so Ctrl+x arrives as `x`.
    Char(char),
    /// Enter.
    Enter,
    /// Escape: close a modal or cancel a running search.
    Escape,
    /// Delete: remove the last character of the field being edited.
    Delete,
    /// Home.
    Home,
    /// End.
    End,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// Nothing happened (poll timeout, resize or an unmapped key); the UI just redraws.
    Tick,
    /// A mouse event, passed through unchanged.
    Mouse(MouseEvent),
}

/// Waits up to `timeout` for a terminal event and translates it.
///
/// Returns [`AppEvent::Tick`] on timeout and on resize, and `None` for events the TUI
/// ignores (key releases, focus changes, paste) or when reading from the terminal fails.
pub fn poll_event(timeout: Duration) -> Option<AppEvent> {
    if !event::poll(timeout).ok()? {
        return Some(AppEvent::Tick);
    }

    match event::read().ok()? {
        Event::Key(key) if key.kind == KeyEventKind::Press => Some(key_to_app_event(key)),
        Event::Mouse(mouse) => Some(AppEvent::Mouse(mouse)),
        Event::Resize(_, _) => Some(AppEvent::Tick),
        _ => None,
    }
}

/// Maps a key press to an [`AppEvent`]; unmapped keys become [`AppEvent::Tick`].
fn key_to_app_event(key: KeyEvent) -> AppEvent {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => AppEvent::Quit,
        KeyCode::Char(c) => AppEvent::Char(c),
        KeyCode::Enter => AppEvent::Enter,
        KeyCode::Esc => AppEvent::Escape,
        KeyCode::Backspace => AppEvent::Back,
        KeyCode::Up => AppEvent::Up,
        KeyCode::Down => AppEvent::Down,
        KeyCode::Left => AppEvent::Left,
        KeyCode::Right => AppEvent::Right,
        KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => AppEvent::BackTab,
        KeyCode::Tab => AppEvent::Tab,
        KeyCode::Delete => AppEvent::Delete,
        KeyCode::Home => AppEvent::Home,
        KeyCode::End => AppEvent::End,
        _ => AppEvent::Tick,
    }
}
