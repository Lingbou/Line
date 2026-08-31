fn profile(id: &str, name: &str) -> Profile {
    Profile {
        id: id.into(),
        name: name.into(),
        host: "example.com".into(),
        port: 22,
        username: "root".into(),
        auth: AuthMethod::Password {
            password: "secret".into(),
        },
    }
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn browse_shortcuts_open_forms_and_connect() {
    let mut app = App::new(vec![profile("1", "one"), profile("2", "two")]);
    assert!(matches!(
        app.handle_key(key(KeyCode::Right, KeyModifiers::NONE)),
        AppAction::None
    ));
    assert_eq!(app.selected_index(), Some(1));
    assert!(
        matches!(app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE)), AppAction::Connect(id) if id == "2")
    );
    app.handle_key(key(KeyCode::Char('t'), KeyModifiers::CONTROL));
    assert_eq!(app.screen(), Screen::Form);
    app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(app.form().expect("edit form").mode, SaveMode::Edit);
    app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert_eq!(app.screen(), Screen::ConfirmDelete);
    assert!(
        matches!(app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE)), AppAction::Delete(id) if id == "2")
    );
}

#[test]
fn unconfigured_browse_shortcuts_do_nothing() {
    let mut app = App::new(vec![profile("1", "one"), profile("2", "two")]);

    for code in [
        KeyCode::Char('h'),
        KeyCode::Char('l'),
        KeyCode::Char('q'),
        KeyCode::Esc,
    ] {
        assert_eq!(
            app.handle_key(key(code, KeyModifiers::NONE)),
            AppAction::None
        );
        assert_eq!(app.selected_index(), Some(0));
        assert_eq!(app.screen(), Screen::Browse);
    }

    assert_eq!(
        app.handle_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        AppAction::Quit
    );
}

#[test]
fn dialogs_accept_only_documented_confirm_and_cancel_keys() {
    let mut app = App::new(vec![profile("1", "one")]);
    app.handle_key(key(KeyCode::Char('d'), KeyModifiers::CONTROL));

    for code in [
        KeyCode::Char('y'),
        KeyCode::Char('Y'),
        KeyCode::Char('n'),
        KeyCode::Char('N'),
    ] {
        assert_eq!(
            app.handle_key(key(code, KeyModifiers::NONE)),
            AppAction::None
        );
        assert_eq!(app.screen(), Screen::ConfirmDelete);
    }
    app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.screen(), Screen::Browse);

    app.set_error("failure");
    assert_eq!(
        app.handle_key(key(KeyCode::Char('q'), KeyModifiers::NONE)),
        AppAction::None
    );
    assert_eq!(app.screen(), Screen::Error);
    app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.screen(), Screen::Browse);
}

#[test]
fn key_source_letters_do_not_change_the_selection() {
    for (code, source) in [
        (KeyCode::Char('i'), KeySource::Paste),
        (KeyCode::Char('e'), KeySource::Paste),
        (KeyCode::Char('p'), KeySource::Import),
    ] {
        let mut app = App::new(Vec::new());
        let form = app.form_mut().unwrap();
        form.auth = AuthDraft::Key {
            source,
            value: String::new(),
            public_key: None,
        };
        form.field = FormField::KeySource;

        assert_eq!(
            app.handle_key(key(code, KeyModifiers::NONE)),
            AppAction::None
        );
        assert!(matches!(
            app.form().unwrap().auth,
            AuthDraft::Key {
                source: unchanged,
                ..
            } if unchanged == source
        ));
    }
}

#[test]
fn form_submission_validates_and_emits_draft() {
    let mut app = App::new(Vec::new());
    app.begin_add();
    {
        let form = app.form_mut().unwrap();
        form.name = "New".into();
        form.host = "root@example.com:2200".into();
        form.username.clear();
        form.auth = AuthDraft::Password {
            password: "pw".into(),
        };
    }
    app.form_mut().unwrap().field = FormField::Host;
    let action = app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        matches!(action, AppAction::Save(draft) if draft.username == "root" && draft.port == 2200)
    );
}

#[test]
fn duplicate_names_are_rejected_case_insensitively() {
    let mut app = App::new(vec![profile("1", "Prod")]);
    app.begin_add();
    let form = app.form_mut().unwrap();
    form.name = "prod".into();
    form.host = "example.org".into();
    form.username = "root".into();
    form.auth = AuthDraft::Password {
        password: "pw".into(),
    };
    assert!(matches!(
        app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE)),
        AppAction::None
    ));
    assert_eq!(
        app.form().unwrap().validation_error.as_deref(),
        Some("A connection with this name already exists")
    );
}

#[test]
fn connection_name_must_be_safe_as_one_key_directory_component() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.name = "prod/root".into();
    form.host = "example.org".into();
    form.username = "root".into();

    assert!(matches!(
        app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE)),
        AppAction::None
    ));
    assert_eq!(
        app.form().unwrap().validation_error.as_deref(),
        Some("Name cannot contain '/' or use a reserved directory name")
    );
}

#[test]
fn paste_event_inserts_multiline_key() {
    let mut app = App::new(Vec::new());
    app.begin_add();
    app.form_mut().unwrap().auth = AuthDraft::Key {
        source: KeySource::Paste,
        value: String::new(),
        public_key: None,
    };
    app.form_mut().unwrap().field = FormField::KeyValue;
    app.handle_event(Event::Paste("PRIVATE\nKEY".into()));
    assert_eq!(app.form().unwrap().current_text(), Some("PRIVATE\nKEY"));
}

#[test]
fn finish_save_and_delete_update_selection() {
    let mut app = App::new(vec![profile("1", "one")]);
    let mut replacement = profile("1", "renamed");
    replacement.port = 2200;
    app.finish_save(replacement);
    assert_eq!(app.selected_profile().unwrap().name, "renamed");
    app.finish_delete("1");
    assert!(app.selected_profile().is_none());
}

#[test]
fn reloading_a_stale_edit_refreshes_the_password_kept_by_a_blank_field() {
    let mut app = App::new(vec![profile("1", "one")]);
    app.begin_edit();
    let mut changed = profile("1", "one");
    changed.auth = AuthMethod::Password {
        password: "changed elsewhere".into(),
    };
    app.set_profiles(vec![changed]);

    app.form_mut().unwrap().field = FormField::Password;
    let action = app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(matches!(
        action,
        AppAction::Save(ProfileDraft {
            auth: AuthDraft::Password { password },
            ..
        }) if password == "changed elsewhere"
    ));
}

#[test]
fn unused_existing_key_requires_confirmation_before_delete_action() {
    let mut app = App::new(Vec::new());
    app.set_available_keys(vec![KeyChoice {
        label: "Key abc".into(),
        private_key: PathBuf::from("keys/.shared/key-abc"),
        public_key: PathBuf::from("keys/.shared/key-abc.pub"),
        used_by: 0,
    }]);
    let form = app.form_mut().unwrap();
    form.auth = AuthDraft::Key {
        source: KeySource::Existing,
        value: "keys/.shared/key-abc".into(),
        public_key: Some("keys/.shared/key-abc.pub".into()),
    };
    form.field = FormField::KeyValue;

    app.handle_key(key(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert_eq!(app.screen(), Screen::ConfirmDeleteKey);
    assert!(matches!(
        app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE)),
        AppAction::DeleteKey(path) if path == Path::new("keys/.shared/key-abc")
    ));
}

#[test]
fn second_tab_after_exact_path_completion_advances_the_form() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.auth = AuthDraft::Key {
        source: KeySource::Import,
        value: "/tmp/id_ed25519".into(),
        public_key: None,
    };
    form.field = FormField::KeyValue;

    assert!(matches!(
        app.handle_key(key(KeyCode::Tab, KeyModifiers::NONE)),
        AppAction::CompletePath(path) if path == "/tmp/id_ed25519"
    ));
    app.finish_path_completion(Ok("/tmp/id_ed25519".into()));
    assert_ne!(app.form().unwrap().field, FormField::KeyValue);
}

#[test]
fn imported_key_accepts_an_optional_public_key_path() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.name = "Imported key".into();
    form.host = "example.org".into();
    form.username = "root".into();
    form.auth = AuthDraft::Key {
        source: KeySource::Import,
        value: "/tmp/private-key".into(),
        public_key: Some("/tmp/public-key".into()),
    };

    assert!(form.fields().contains(&FormField::PublicKey));
    form.field = FormField::PublicKey;
    let action = app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(matches!(
        action,
        AppAction::Save(ProfileDraft {
            auth: AuthDraft::Key {
                source: KeySource::Import,
                value,
                public_key: Some(public_key),
            },
            ..
        }) if value == "/tmp/private-key" && public_key == "/tmp/public-key"
    ));
}

#[test]
fn path_completion_targets_the_active_import_path() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.auth = AuthDraft::Key {
        source: KeySource::Import,
        value: "/tmp/private-key".into(),
        public_key: Some("/tmp/public-key".into()),
    };
    form.field = FormField::PublicKey;

    assert!(matches!(
        app.handle_key(key(KeyCode::Tab, KeyModifiers::NONE)),
        AppAction::CompletePath(path) if path == "/tmp/public-key"
    ));
    app.finish_path_completion(Ok("/tmp/public-key-completed".into()));
    assert!(matches!(
        &app.form().unwrap().auth,
        AuthDraft::Key {
            public_key: Some(public_key),
            ..
        } if public_key == "/tmp/public-key-completed"
    ));
}

#[test]
fn password_paste_drops_one_clipboard_line_ending_but_preserves_spaces() {
    let mut app = App::new(Vec::new());
    app.form_mut().unwrap().field = FormField::Password;
    app.handle_event(Event::Paste("  secret value  \r\n".into()));

    assert!(matches!(
        &app.form().unwrap().auth,
        AuthDraft::Password { password } if password == "  secret value  "
    ));
}
use std::path::{Path, PathBuf};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::config::{AuthMethod, Profile};

use super::*;
