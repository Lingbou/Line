use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use super::{
    App, AppAction, AuthDraft, FormField, KeySource, MouseTarget, Screen,
    helpers::{insert_char, insert_str, path_to_string, remove_char},
};

impl App {
    pub fn handle_event(&mut self, event: Event) -> AppAction {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::Paste(text) => self.handle_paste(&text),
            _ => AppAction::None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return AppAction::Quit;
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

    fn handle_browse_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Left => self.move_selection(-1),
            KeyCode::Right => self.move_selection(1),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
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
            _ => {}
        }
        AppAction::None
    }

    fn connect_selected(&mut self) -> AppAction {
        self.selected_profile()
            .map(|profile| AppAction::Connect(profile.id.clone()))
            .unwrap_or(AppAction::None)
    }

    fn handle_form_key(&mut self, key: KeyEvent) -> AppAction {
        if key.code == KeyCode::Esc {
            self.form = None;
            self.form_conflict = false;
            self.screen = Screen::Browse;
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
                if value.trim().is_empty()
                    && self
                        .form
                        .as_ref()
                        .is_some_and(|form| form.field == FormField::PublicKey)
                {
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
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => {
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

    fn handle_text_key(&mut self, key: KeyEvent) -> Option<AppAction> {
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(form) = self.form.as_mut() {
                    let cursor = form.cursor;
                    if let Some(text) = form.current_text_mut() {
                        insert_char(text, cursor, c);
                        form.cursor = cursor + 1;
                        form.validation_error = None;
                    }
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
            KeyCode::Delete => {
                if let Some(form) = self.form.as_mut() {
                    let cursor = form.cursor;
                    if let Some(text) = form.current_text_mut() {
                        remove_char(text, cursor);
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

    fn handle_paste(&mut self, text: &str) -> AppAction {
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
        let text = if form.field == FormField::Password {
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

    pub(super) fn form_next(&mut self, backwards: bool) {
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

    fn set_key_source(&mut self, source: KeySource) {
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

    fn cycle_key_source(&mut self, delta: i32) {
        let source = self.form.as_ref().and_then(|form| match form.auth {
            AuthDraft::Key { source, .. } => Some(source.next(delta)),
            _ => None,
        });
        if let Some(source) = source {
            self.set_key_source(source);
        }
    }

    pub(super) fn ensure_existing_key_selected(&mut self) {
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

    fn select_existing_key(&mut self, delta: i32) {
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

    fn submit_form(&mut self) -> AppAction {
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

    fn handle_confirm_key(&mut self, key: KeyEvent) -> AppAction {
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

    fn confirm_delete(&mut self) -> AppAction {
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

    fn begin_delete_key(&mut self) {
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

    fn confirm_delete_key(&mut self) -> AppAction {
        self.delete_key_target
            .clone()
            .map(AppAction::DeleteKey)
            .unwrap_or(AppAction::None)
    }

    fn dismiss_error(&mut self) {
        self.error_message = None;
        self.error_scroll = 0;
        self.screen = self.error_return_screen;
    }

    fn error_scroll_limit(&self) -> u16 {
        self.error_message
            .as_deref()
            .map(|message| {
                message
                    .lines()
                    .map(|line| line.chars().count().max(1).div_ceil(60))
                    .sum::<usize>()
                    .saturating_sub(8)
                    .min(u16::MAX as usize) as u16
            })
            .unwrap_or(0)
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) -> AppAction {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(target) = self.mouse_regions.target_at(mouse.column, mouse.row) {
                    return self.handle_mouse_target(target);
                }
            }
            MouseEventKind::ScrollUp => match self.screen {
                Screen::Browse => self.move_selection(-1),
                Screen::Error => self.error_scroll = self.error_scroll.saturating_sub(1),
                Screen::Form if self.is_choosing_existing_key() => self.select_existing_key(-1),
                _ => {}
            },
            MouseEventKind::ScrollDown => match self.screen {
                Screen::Browse => self.move_selection(1),
                Screen::Error => {
                    self.error_scroll = self
                        .error_scroll
                        .saturating_add(1)
                        .min(self.error_scroll_limit())
                }
                Screen::Form if self.is_choosing_existing_key() => self.select_existing_key(1),
                _ => {}
            },
            _ => {}
        }
        AppAction::None
    }

    pub fn handle_mouse_target(&mut self, target: MouseTarget) -> AppAction {
        match self.screen {
            Screen::Browse => match target {
                MouseTarget::Tab(index) => self.select(index),
                MouseTarget::Connect => return self.connect_selected(),
                MouseTarget::Add => self.begin_add(),
                MouseTarget::Edit => {
                    self.begin_edit();
                }
                MouseTarget::Delete => {
                    self.begin_delete();
                }
                _ => {}
            },
            Screen::Form => match target {
                MouseTarget::Field(field) => {
                    if let Some(form) = self.form.as_mut() {
                        form.set_field(field);
                    }
                }
                MouseTarget::AuthPassword => {
                    if let Some(form) = self.form.as_mut() {
                        form.auth = AuthDraft::Password {
                            password: match &form.auth {
                                AuthDraft::Password { password } => password.clone(),
                                _ => String::new(),
                            },
                        };
                        form.set_field(FormField::Authentication);
                    }
                }
                MouseTarget::AuthKey => {
                    if let Some(form) = self.form.as_mut()
                        && !matches!(form.auth, AuthDraft::Key { .. })
                    {
                        form.auth = AuthDraft::Key {
                            source: KeySource::Existing,
                            value: String::new(),
                            public_key: None,
                        };
                        form.set_field(FormField::Authentication);
                    }
                    self.ensure_existing_key_selected();
                }
                MouseTarget::KeyImport => self.set_key_source(KeySource::Import),
                MouseTarget::KeyExisting => self.set_key_source(KeySource::Existing),
                MouseTarget::KeyPaste => self.set_key_source(KeySource::Paste),
                MouseTarget::DeleteKey => self.begin_delete_key(),
                MouseTarget::ShowPassword => {
                    if let Some(form) = self.form.as_mut() {
                        form.show_password = !form.show_password;
                    }
                }
                MouseTarget::Save => return self.submit_form(),
                MouseTarget::Cancel => {
                    self.form = None;
                    self.form_conflict = false;
                    self.screen = Screen::Browse;
                }
                _ => {}
            },
            Screen::ConfirmDelete => match target {
                MouseTarget::Confirm => return self.confirm_delete(),
                MouseTarget::Cancel => {
                    self.delete_target = None;
                    self.screen = Screen::Browse;
                }
                _ => {}
            },
            Screen::ConfirmDeleteKey => match target {
                MouseTarget::Confirm => return self.confirm_delete_key(),
                MouseTarget::Cancel => {
                    self.delete_key_target = None;
                    self.screen = Screen::Form;
                }
                _ => {}
            },
            Screen::Error => {
                if matches!(target, MouseTarget::Dismiss | MouseTarget::Cancel) {
                    self.dismiss_error();
                }
            }
        }
        AppAction::None
    }

    fn is_choosing_existing_key(&self) -> bool {
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
