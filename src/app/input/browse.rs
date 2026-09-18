use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppAction};

impl App {
    pub(super) fn handle_browse_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Left | KeyCode::Up => self.move_selection(-1),
            KeyCode::Right | KeyCode::Down => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-5),
            KeyCode::PageDown => self.move_selection(5),
            KeyCode::Home => {
                if let Some(&first) = self.visible_profiles.first() {
                    self.select(first);
                }
            }
            KeyCode::End => {
                if let Some(&last) = self.visible_profiles.last() {
                    self.select(last);
                }
            }
            KeyCode::Enter => return self.connect_selected(),
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.begin_add();
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.begin_edit();
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.begin_delete();
            }
            KeyCode::Backspace
                if key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.backspace_word_browse_query();
            }
            KeyCode::Backspace => self.backspace_browse_query(),
            KeyCode::Char('w' | 'h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.backspace_word_browse_query();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.clear_browse_query();
            }
            KeyCode::Esc => self.clear_browse_query(),
            KeyCode::Char('/') if self.browse_query.is_empty() => {}
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && !character.is_control() =>
            {
                self.append_browse_query(character.encode_utf8(&mut [0; 4]));
            }
            _ => {}
        }
        AppAction::None
    }

    pub(super) fn connect_selected(&mut self) -> AppAction {
        self.selected_profile()
            .map(|profile| AppAction::Connect(profile.id.clone()))
            .unwrap_or(AppAction::None)
    }
}
