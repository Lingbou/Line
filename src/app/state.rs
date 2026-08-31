use std::path::{Path, PathBuf};

use crate::config::{AuthMethod, Profile};

use super::{
    AuthDraft, FormField, FormState, KeyChoice, KeySource, MouseRegions, Screen,
    helpers::{next_default_name, path_to_string},
};

/// Complete TUI state.  It is intentionally cheap to clone for tests and is
/// free of terminal/backend handles.
pub struct App {
    pub(super) profiles: Vec<Profile>,
    pub(super) selected: usize,
    pub(super) screen: Screen,
    pub(super) form: Option<FormState>,
    pub(super) form_conflict: bool,
    pub(super) delete_target: Option<usize>,
    pub(super) delete_key_target: Option<PathBuf>,
    pub(super) error_message: Option<String>,
    pub(super) error_scroll: u16,
    pub(super) error_return_screen: Screen,
    pub(super) status_message: Option<String>,
    pub(super) mouse_regions: MouseRegions,
    pub(super) available_keys: Vec<KeyChoice>,
}

impl std::fmt::Debug for App {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("App")
            .field("profiles", &self.profiles)
            .field("selected", &self.selected)
            .field("screen", &self.screen)
            .field("form", &self.form)
            .field("delete_target", &self.delete_target)
            .field("error_message", &self.error_message)
            .field("status_message", &self.status_message)
            .finish()
    }
}

impl App {
    pub fn new(profiles: Vec<Profile>) -> Self {
        let selected = 0;
        let mut app = Self {
            profiles,
            selected,
            screen: Screen::Browse,
            form: None,
            form_conflict: false,
            delete_target: None,
            delete_key_target: None,
            error_message: None,
            error_scroll: 0,
            error_return_screen: Screen::Browse,
            status_message: None,
            mouse_regions: MouseRegions::default(),
            available_keys: Vec::new(),
        };
        if app.profiles.is_empty() {
            app.begin_add();
        }
        app
    }

    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    pub fn selected_index(&self) -> Option<usize> {
        if self.profiles.is_empty() {
            None
        } else {
            Some(self.selected.min(self.profiles.len() - 1))
        }
    }

    pub fn selected_profile(&self) -> Option<&Profile> {
        self.selected_index()
            .and_then(|index| self.profiles.get(index))
    }

    pub fn screen(&self) -> Screen {
        self.screen
    }

    pub fn form(&self) -> Option<&FormState> {
        self.form.as_ref()
    }

    pub fn form_mut(&mut self) -> Option<&mut FormState> {
        self.form.as_mut()
    }

    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    pub fn error_scroll(&self) -> u16 {
        self.error_scroll
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }

    pub fn delete_target_profile(&self) -> Option<&Profile> {
        self.delete_target
            .and_then(|index| self.profiles.get(index))
    }

    pub fn delete_key_target(&self) -> Option<&KeyChoice> {
        let target = self.delete_key_target.as_ref()?;
        self.available_keys
            .iter()
            .find(|key| &key.private_key == target)
    }

    pub fn available_keys(&self) -> &[KeyChoice] {
        &self.available_keys
    }

    pub fn set_available_keys(&mut self, keys: Vec<KeyChoice>) {
        self.available_keys = keys;
        self.ensure_existing_key_selected();
    }

    pub fn set_mouse_regions(&mut self, regions: MouseRegions) {
        self.mouse_regions = regions;
    }

    pub fn set_profiles(&mut self, profiles: Vec<Profile>) {
        self.profiles = profiles;
        if let Some(form) = self.form.as_mut()
            && let Some(id) = form.profile_id.as_deref()
        {
            form.original_password = self
                .profiles
                .iter()
                .find(|profile| profile.id == id)
                .and_then(|profile| match &profile.auth {
                    AuthMethod::Password { password } => Some(password.clone()),
                    AuthMethod::Key { .. } => None,
                });
        }
        if self.profiles.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.profiles.len() - 1);
        }
    }

    pub fn select(&mut self, index: usize) {
        if !self.profiles.is_empty() {
            self.selected = index.min(self.profiles.len() - 1);
        }
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.profiles.is_empty() {
            return;
        }
        self.selected =
            (self.selected as i32 + delta).rem_euclid(self.profiles.len() as i32) as usize;
    }

    pub fn begin_add(&mut self) {
        let default_name = next_default_name(&self.profiles);
        self.form = Some(FormState::add(default_name));
        self.form_conflict = false;
        self.screen = Screen::Form;
        self.status_message = None;
    }

    pub fn begin_edit(&mut self) -> bool {
        let Some(profile) = self.selected_profile().cloned() else {
            return false;
        };
        self.form = Some(FormState::edit(&profile));
        self.form_conflict = false;
        self.screen = Screen::Form;
        self.status_message = None;
        true
    }

    pub fn begin_delete(&mut self) -> bool {
        if self.selected_profile().is_none() {
            return false;
        }
        self.delete_target = self.selected_index();
        self.screen = Screen::ConfirmDelete;
        true
    }

    /// Complete a save operation performed by the adapter.
    pub fn finish_save(&mut self, profile: Profile) {
        if let Some(index) = self.profiles.iter().position(|item| item.id == profile.id) {
            self.profiles[index] = profile;
            self.selected = index;
        } else {
            self.profiles.push(profile);
            self.selected = self.profiles.len() - 1;
        }
        self.form = None;
        self.form_conflict = false;
        self.screen = Screen::Browse;
        self.status_message = Some("Saved connection".into());
        self.error_message = None;
    }

    /// Complete a delete operation performed by the adapter.
    pub fn finish_delete(&mut self, id: &str) {
        if let Some(index) = self.profiles.iter().position(|item| item.id == id) {
            self.profiles.remove(index);
            if self.profiles.is_empty() {
                self.selected = 0;
            } else {
                self.selected = self.selected.min(self.profiles.len() - 1);
            }
        }
        self.delete_target = None;
        self.form_conflict = false;
        self.screen = Screen::Browse;
        self.status_message = Some("Deleted connection".into());
    }

    pub fn cancel_delete(&mut self) {
        self.delete_target = None;
        self.screen = Screen::Browse;
    }

    pub fn finish_delete_key(&mut self, private_key: &Path) {
        self.available_keys
            .retain(|key| key.private_key != private_key);
        self.delete_key_target = None;
        self.screen = Screen::Form;
        self.status_message = Some("Deleted unused key".into());

        let replacement = self.available_keys.first().cloned();
        if let Some(form) = self.form.as_mut()
            && let AuthDraft::Key {
                source: KeySource::Existing,
                value,
                public_key,
            } = &mut form.auth
        {
            if let Some(key) = replacement {
                *value = path_to_string(&key.private_key);
                *public_key = Some(path_to_string(&key.public_key));
            } else {
                value.clear();
                *public_key = None;
            }
        }
    }

    pub fn cancel_delete_key(&mut self) {
        self.delete_key_target = None;
        self.screen = Screen::Form;
    }

    /// Return to the browse screen after an adapter rejected a save/delete.
    pub fn set_error(&mut self, message: impl Into<String>) {
        if self.screen != Screen::Error {
            self.error_return_screen = self.screen;
        }
        let message = message.into();
        if self.form.is_some() && message.contains("changed in another line process") {
            self.form_conflict = true;
            if let Some(form) = self.form.as_mut() {
                form.validation_error = Some(
                    "Connection changed elsewhere; cancel and edit it again before saving.".into(),
                );
            }
        }
        self.error_scroll = 0;
        self.error_message = Some(message);
        self.screen = Screen::Error;
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    /// Apply a path completion produced by the filesystem adapter.
    pub fn finish_path_completion(&mut self, result: Result<String, String>) {
        let mut advance = false;
        let Some(form) = self.form.as_mut() else {
            return;
        };
        match result {
            Ok(value) => {
                if let AuthDraft::Key {
                    source: KeySource::Import,
                    value: private_key,
                    public_key,
                } = &mut form.auth
                {
                    let current = match form.field {
                        FormField::KeyValue => Some(private_key),
                        FormField::PublicKey => Some(public_key.get_or_insert_with(String::new)),
                        _ => None,
                    };
                    if let Some(current) = current {
                        advance = *current == value;
                        *current = value;
                        form.cursor = current.chars().count();
                        form.validation_error = None;
                    }
                }
            }
            Err(error) => form.validation_error = Some(error),
        }
        if advance {
            self.form_next(false);
        }
    }
}
