use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use super::{App, AppAction, Screen};

mod actions;
mod browse;
mod form;
mod keys;
mod mouse;
mod text;

impl App {
    pub fn handle_event(&mut self, event: Event) -> AppAction {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::Paste(text) if !self.terminal_too_small => self.handle_paste(&text),
            Event::Paste(_) => AppAction::None,
            _ => AppAction::None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return AppAction::Quit;
        }

        // If the active controls are not visible, Enter must not connect,
        // save, or confirm deletion. Escape remains available so the user can
        // back out of the hidden screen.
        if self.terminal_too_small && key.code != KeyCode::Esc {
            return AppAction::None;
        }

        match self.screen {
            Screen::Browse => self.handle_browse_key(key),
            Screen::Form => self.handle_form_key(key),
            Screen::ConfirmDelete | Screen::ConfirmDeleteKey => self.handle_confirm_key(key),
            Screen::Error => {
                let limit = self.error_scroll_limit();
                match key.code {
                    KeyCode::Up => self.error_scroll = self.error_scroll.saturating_sub(1),
                    KeyCode::PageUp => self.error_scroll = self.error_scroll.saturating_sub(6),
                    KeyCode::Down => {
                        self.error_scroll = self.error_scroll.saturating_add(1).min(limit)
                    }
                    KeyCode::PageDown => {
                        self.error_scroll = self.error_scroll.saturating_add(6).min(limit)
                    }
                    KeyCode::Enter | KeyCode::Esc => self.dismiss_error(),
                    _ => {}
                }
                AppAction::None
            }
        }
    }
}
