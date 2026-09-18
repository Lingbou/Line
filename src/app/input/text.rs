use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{
    App, AppAction, AuthDraft, FormField, KeySource, Screen,
    helpers::{
        insert_char, insert_str, next_word_boundary, prev_word_boundary, remove_char,
        remove_char_range,
    },
};

impl App {
    pub(super) fn handle_text_key(&mut self, key: KeyEvent) -> Option<AppAction> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char('a') if ctrl => {
                if let Some(form) = self.form.as_mut() {
                    form.cursor = 0;
                }
                Some(AppAction::None)
            }
            KeyCode::Char('e') if ctrl => {
                if let Some(form) = self.form.as_mut() {
                    form.cursor = form
                        .current_text()
                        .map(|text| text.chars().count())
                        .unwrap_or(0);
                }
                Some(AppAction::None)
            }
            KeyCode::Char('u') if ctrl => {
                if let Some(form) = self.form.as_mut() {
                    let cursor = form.cursor;
                    if cursor > 0
                        && let Some(text) = form.current_text_mut()
                    {
                        remove_char_range(text, 0, cursor);
                        form.cursor = 0;
                        form.validation_error = None;
                    }
                }
                Some(AppAction::None)
            }
            KeyCode::Char('k') if ctrl => {
                if let Some(form) = self.form.as_mut() {
                    let cursor = form.cursor;
                    if let Some(text) = form.current_text_mut() {
                        let len = text.chars().count();
                        if cursor < len {
                            remove_char_range(text, cursor, len);
                            form.validation_error = None;
                        }
                    }
                }
                Some(AppAction::None)
            }
            // Handle both Ctrl+W and Ctrl+Backspace.
            // On standard Unix terminals (Konsole, xterm, gnome-terminal), Ctrl+Backspace
            // transmits ASCII 0x08 (^H), which crossterm decodes as Char('h') with CONTROL.
            KeyCode::Char('w' | 'h') if ctrl => {
                if let Some(form) = self.form.as_mut() {
                    form.delete_prev_word();
                }
                Some(AppAction::None)
            }
            KeyCode::Char('d') if alt => {
                if let Some(form) = self.form.as_mut() {
                    form.delete_next_word();
                }
                Some(AppAction::None)
            }
            KeyCode::Char(character) if !ctrl => {
                if let Some(form) = self.form.as_mut() {
                    if form.field == FormField::Port && !character.is_ascii_digit() {
                        return Some(AppAction::None);
                    }
                    let cursor = form.cursor;
                    if let Some(text) = form.current_text_mut() {
                        insert_char(text, cursor, character);
                        form.cursor = cursor + 1;
                        form.validation_error = None;
                    }
                }
                Some(AppAction::None)
            }
            KeyCode::Backspace if ctrl || alt => {
                if let Some(form) = self.form.as_mut() {
                    form.delete_prev_word();
                }
                Some(AppAction::None)
            }
            KeyCode::Backspace => {
                if let Some(form) = self.form.as_mut() {
                    let cursor = form.cursor;
                    if cursor > 0
                        && let Some(text) = form.current_text_mut()
                    {
                        remove_char(text, cursor - 1);
                        form.cursor = cursor - 1;
                    }
                }
                Some(AppAction::None)
            }
            KeyCode::Delete if ctrl || alt => {
                if let Some(form) = self.form.as_mut() {
                    form.delete_next_word();
                }
                Some(AppAction::None)
            }
            KeyCode::Delete => {
                if let Some(form) = self.form.as_mut() {
                    let cursor = form.cursor;
                    if let Some(text) = form.current_text_mut() {
                        remove_char(text, cursor);
                    }
                }
                Some(AppAction::None)
            }
            KeyCode::Left if ctrl || alt => {
                if let Some(form) = self.form.as_mut() {
                    let cursor = form.cursor;
                    if let Some(text) = form.current_text() {
                        form.cursor = prev_word_boundary(text, cursor);
                    }
                }
                Some(AppAction::None)
            }
            KeyCode::Left => {
                if let Some(form) = self.form.as_mut() {
                    form.cursor = form.cursor.saturating_sub(1);
                }
                Some(AppAction::None)
            }
            KeyCode::Right if ctrl || alt => {
                if let Some(form) = self.form.as_mut() {
                    let cursor = form.cursor;
                    if let Some(text) = form.current_text() {
                        form.cursor = next_word_boundary(text, cursor);
                    }
                }
                Some(AppAction::None)
            }
            KeyCode::Right => {
                if let Some(form) = self.form.as_mut() {
                    let max = form
                        .current_text()
                        .map(|text| text.chars().count())
                        .unwrap_or(0);
                    form.cursor = (form.cursor + 1).min(max);
                }
                Some(AppAction::None)
            }
            KeyCode::Home => {
                if let Some(form) = self.form.as_mut() {
                    form.cursor = 0;
                }
                Some(AppAction::None)
            }
            KeyCode::End => {
                if let Some(form) = self.form.as_mut() {
                    form.cursor = form
                        .current_text()
                        .map(|text| text.chars().count())
                        .unwrap_or(0);
                }
                Some(AppAction::None)
            }
            KeyCode::Enter => None,
            _ => Some(AppAction::None),
        }
    }

    pub(super) fn handle_paste(&mut self, text: &str) -> AppAction {
        if self.screen == Screen::Browse {
            self.append_browse_query(text);
            return AppAction::None;
        }
        if self.screen != Screen::Form {
            return AppAction::None;
        }
        let Some(form) = self.form.as_mut() else {
            return AppAction::None;
        };
        if matches!(
            (&form.auth, form.field),
            (
                AuthDraft::Key {
                    source: KeySource::Existing,
                    ..
                },
                FormField::KeyValue
            )
        ) {
            return AppAction::None;
        }
        if !form.field.is_text() {
            return AppAction::None;
        }

        let filtered;
        let text = if form.field == FormField::Port {
            filtered = text
                .chars()
                .filter(char::is_ascii_digit)
                .collect::<String>();
            filtered.as_str()
        } else if form.field == FormField::Password {
            text.strip_suffix("\r\n")
                .or_else(|| text.strip_suffix('\n'))
                .or_else(|| text.strip_suffix('\r'))
                .unwrap_or(text)
        } else {
            text
        };
        let cursor = form.cursor;
        if let Some(value) = form.current_text_mut() {
            insert_str(value, cursor, text);
            form.cursor = cursor + text.chars().count();
            form.validation_error = None;
        }
        AppAction::None
    }

    pub(crate) fn form_next(&mut self, backwards: bool) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let fields = form.fields();
        let index = fields
            .iter()
            .position(|field| *field == form.field)
            .unwrap_or(0);
        let delta = if backwards { -1 } else { 1 };
        let next = (index as i32 + delta).rem_euclid(fields.len() as i32) as usize;
        form.set_field(fields[next]);
    }
}
