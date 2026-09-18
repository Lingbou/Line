use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppAction, AuthDraft, FormField, KeySource};

impl App {
    pub(super) fn handle_form_key(&mut self, key: KeyEvent) -> AppAction {
        if key.code == KeyCode::Esc {
            self.form = None;
            self.form_conflict = false;
            self.screen = crate::app::Screen::Browse;
            return AppAction::None;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('d')
            && matches!(
                self.form.as_ref().map(|form| (&form.auth, form.field)),
                Some((
                    AuthDraft::Key {
                        source: KeySource::Existing,
                        ..
                    },
                    FormField::KeyValue
                ))
            )
        {
            self.begin_delete_key();
            return AppAction::None;
        }
        if key.code == KeyCode::Tab && !key.modifiers.contains(KeyModifiers::SHIFT) {
            let import_path = self.form.as_ref().and_then(|form| {
                let AuthDraft::Key {
                    source: KeySource::Import,
                    value,
                    public_key,
                } = &form.auth
                else {
                    return None;
                };
                match form.field {
                    FormField::KeyValue => Some(value.clone()),
                    FormField::PublicKey => Some(public_key.clone().unwrap_or_default()),
                    _ => None,
                }
            });
            if let Some(value) = import_path {
                if value.trim().is_empty() {
                    self.form_next(false);
                    return AppAction::None;
                }
                return AppAction::CompletePath(value);
            }
        }
        if key.code == KeyCode::Tab {
            self.form_next(key.modifiers.contains(KeyModifiers::SHIFT));
            return AppAction::None;
        }
        if key.code == KeyCode::BackTab {
            self.form_next(true);
            return AppAction::None;
        }

        let field = self.form.as_ref().map(|form| form.field);
        let choosing_existing = matches!(
            self.form.as_ref().map(|form| (&form.auth, form.field)),
            Some((
                AuthDraft::Key {
                    source: KeySource::Existing,
                    ..
                },
                FormField::KeyValue
            ))
        );
        if choosing_existing {
            match key.code {
                KeyCode::Left | KeyCode::Up => {
                    self.select_existing_key(-1);
                    return AppAction::None;
                }
                KeyCode::Right | KeyCode::Down | KeyCode::Char(' ') => {
                    self.select_existing_key(1);
                    return AppAction::None;
                }
                KeyCode::Enter => {}
                _ => return AppAction::None,
            }
        }
        match field {
            Some(FormField::Authentication) => match key.code {
                KeyCode::Left => {
                    self.form_mut().expect("form exists").toggle_auth(-1);
                    self.ensure_existing_key_selected();
                    return AppAction::None;
                }
                KeyCode::Right | KeyCode::Char(' ') => {
                    self.form_mut().expect("form exists").toggle_auth(1);
                    self.ensure_existing_key_selected();
                    return AppAction::None;
                }
                _ => {}
            },
            Some(FormField::ShowPassword) => match key.code {
                KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => {
                    if let Some(form) = self.form.as_mut() {
                        form.show_password = !form.show_password;
                    }
                    return AppAction::None;
                }
                _ => {}
            },
            Some(FormField::KeySource) => match key.code {
                KeyCode::Left => {
                    self.cycle_key_source(-1);
                    return AppAction::None;
                }
                KeyCode::Right | KeyCode::Char(' ') => {
                    self.cycle_key_source(1);
                    return AppAction::None;
                }
                _ => {}
            },
            Some(field) if field.is_text() && !matches!(key.code, KeyCode::Up | KeyCode::Down) => {
                if let Some(action) = self.handle_text_key(key) {
                    return action;
                }
            }
            _ => {}
        }

        match key.code {
            KeyCode::Enter => self.submit_form(),
            KeyCode::Down | KeyCode::Right => {
                // Right/Down on non-selector fields behaves like Tab, making
                // the form comfortable with arrows alone.
                self.form_next(false);
                AppAction::None
            }
            KeyCode::Up | KeyCode::Left => {
                self.form_next(true);
                AppAction::None
            }
            _ => AppAction::None,
        }
    }
}
