use crate::app::{App, AuthDraft, FormField, KeySource, helpers::path_to_string};

impl App {
    pub(super) fn set_key_source(&mut self, source: KeySource) {
        if let Some(form) = self.form.as_mut()
            && let AuthDraft::Key {
                source: current,
                value,
                public_key,
            } = &mut form.auth
        {
            if *current != source {
                *current = source;
                value.clear();
                *public_key = None;
            }
            form.set_field(FormField::KeySource);
        }
        if source == KeySource::Existing {
            self.ensure_existing_key_selected();
        }
    }

    pub(super) fn cycle_key_source(&mut self, delta: i32) {
        let source = self.form.as_ref().and_then(|form| match form.auth {
            AuthDraft::Key { source, .. } => Some(source.next(delta)),
            _ => None,
        });
        if let Some(source) = source {
            self.set_key_source(source);
        }
    }

    pub(super) fn cycle_jump(&mut self, delta: i32) {
        let profiles = self.profiles.clone();
        if let Some(form) = self.form.as_mut() {
            form.jump.cycle(&profiles, delta);
        }
    }

    pub(crate) fn ensure_existing_key_selected(&mut self) {
        let first = self.available_keys.first().cloned();
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let AuthDraft::Key {
            source: KeySource::Existing,
            value,
            public_key,
        } = &mut form.auth
        else {
            return;
        };
        let is_known = self
            .available_keys
            .iter()
            .any(|key| path_to_string(&key.private_key) == *value);
        if !is_known && let Some(key) = first {
            *value = path_to_string(&key.private_key);
            *public_key = Some(path_to_string(&key.public_key));
        }
    }

    pub(super) fn select_existing_key(&mut self, delta: i32) {
        if self.available_keys.is_empty() {
            return;
        }
        let current_value = self.form.as_ref().and_then(|form| match &form.auth {
            AuthDraft::Key { value, .. } => Some(value.as_str()),
            _ => None,
        });
        let current = current_value
            .and_then(|value| {
                self.available_keys
                    .iter()
                    .position(|key| path_to_string(&key.private_key) == value)
            })
            .unwrap_or(if delta < 0 {
                0
            } else {
                self.available_keys.len() - 1
            });
        let index = (current as i32 + delta).rem_euclid(self.available_keys.len() as i32) as usize;
        let key = self.available_keys[index].clone();
        if let Some(form) = self.form.as_mut()
            && let AuthDraft::Key {
                value, public_key, ..
            } = &mut form.auth
        {
            *value = path_to_string(&key.private_key);
            *public_key = Some(path_to_string(&key.public_key));
        }
    }

    pub(super) fn is_choosing_existing_key(&self) -> bool {
        matches!(
            self.form.as_ref().map(|form| (&form.auth, form.field)),
            Some((
                AuthDraft::Key {
                    source: KeySource::Existing,
                    ..
                },
                FormField::KeyValue
            ))
        )
    }
}
