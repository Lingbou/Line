use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, AppAction, AuthDraft, KeySource, Screen, helpers::path_to_string};

impl App {
    pub(super) fn submit_form(&mut self) -> AppAction {
        if self.form_conflict {
            if let Some(form) = self.form.as_mut() {
                form.validation_error = Some(
                    "Connection changed elsewhere; cancel and edit it again before saving.".into(),
                );
            }
            return AppAction::None;
        }
        let Some(form) = self.form.as_ref() else {
            return AppAction::None;
        };
        match form.draft(&self.profiles) {
            Ok(draft) => {
                if let Some(form) = self.form.as_mut() {
                    form.validation_error = None;
                }
                AppAction::Save(draft)
            }
            Err(error) => {
                if let Some(form) = self.form.as_mut() {
                    form.validation_error = Some(error);
                }
                AppAction::None
            }
        }
    }

    pub(super) fn handle_confirm_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Enter => {
                if self.screen == Screen::ConfirmDeleteKey {
                    self.confirm_delete_key()
                } else {
                    self.confirm_delete()
                }
            }
            KeyCode::Esc => {
                if self.screen == Screen::ConfirmDeleteKey {
                    self.delete_key_target = None;
                    self.screen = Screen::Form;
                } else {
                    self.delete_target = None;
                    self.screen = Screen::Browse;
                }
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    pub(super) fn confirm_delete(&mut self) -> AppAction {
        let Some(index) = self.delete_target else {
            self.screen = Screen::Browse;
            return AppAction::None;
        };
        let Some(profile) = self.profiles.get(index).cloned() else {
            self.screen = Screen::Browse;
            return AppAction::None;
        };
        AppAction::Delete(profile.id)
    }

    pub(super) fn begin_delete_key(&mut self) {
        let selected = self.form.as_ref().and_then(|form| match &form.auth {
            AuthDraft::Key {
                source: KeySource::Existing,
                value,
                ..
            } => self
                .available_keys
                .iter()
                .find(|key| path_to_string(&key.private_key) == *value)
                .cloned(),
            _ => None,
        });
        let Some(key) = selected else {
            if let Some(form) = self.form.as_mut() {
                form.validation_error = Some("Choose an existing key before deleting it".into());
            }
            return;
        };
        if key.used_by > 0 {
            if let Some(form) = self.form.as_mut() {
                form.validation_error = Some(format!(
                    "This key is still used by {} connection{}",
                    key.used_by,
                    if key.used_by == 1 { "" } else { "s" }
                ));
            }
            return;
        }
        self.delete_key_target = Some(key.private_key);
        self.screen = Screen::ConfirmDeleteKey;
    }

    pub(super) fn confirm_delete_key(&mut self) -> AppAction {
        self.delete_key_target
            .clone()
            .map(AppAction::DeleteKey)
            .unwrap_or(AppAction::None)
    }

    pub(super) fn dismiss_error(&mut self) {
        self.error_message = None;
        self.error_scroll = 0;
        self.error_scroll_limit = 0;
        self.screen = self.error_return_screen;
    }

    pub(super) fn error_scroll_limit(&self) -> u16 {
        self.error_scroll_limit
    }
}
