use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::app::{App, AppAction, AuthDraft, FormField, KeySource, MouseTarget, Screen};

impl App {
    pub fn handle_mouse(&mut self, mouse: MouseEvent) -> AppAction {
        if self.terminal_too_small {
            return AppAction::None;
        }
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
        if self.terminal_too_small {
            return AppAction::None;
        }
        match self.screen {
            Screen::Browse => match target {
                MouseTarget::Profile(index) => self.select(index),
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
}
