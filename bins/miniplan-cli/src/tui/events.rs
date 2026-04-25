use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    Quit,
    Back,
    Up,
    Down,
    Tab,
    BackTab,
    Char(char),
    Enter,
    Escape,
    Delete,
    Home,
    End,
    Left,
    Right,
    Tick,
    Mouse(MouseEvent),
}

pub fn poll_event(timeout: Duration) -> Option<AppEvent> {
    if event::poll(timeout).ok()? {
        match event::read().ok()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => Some(key_to_app_event(key)),
            Event::Mouse(mouse) => Some(AppEvent::Mouse(mouse)),
            Event::Resize(_, _) => Some(AppEvent::Tick),
            _ => None,
        }
    } else {
        Some(AppEvent::Tick)
    }
}

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
