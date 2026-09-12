use std::path::{Path, PathBuf};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::config::{AuthMethod, Profile};

use super::*;

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
fn ordinary_browse_letters_filter_instead_of_acting_as_shortcuts() {
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
        assert_eq!(app.screen(), Screen::Browse);
    }
    assert_eq!(app.browse_query(), "");
    assert_eq!(app.selected_index(), Some(0));

    assert_eq!(
        app.handle_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        AppAction::Quit
    );
}

#[test]
fn browse_filter_matches_names_usernames_and_hosts_case_insensitively() {
    let mut deployment = profile("2", "生产环境");
    deployment.username = "Deploy".into();
    deployment.host = "10.20.30.40".into();
    let mut app = App::new(vec![profile("1", "Staging"), deployment]);

    for (query, expected) in [("staG", 0), ("生产", 1), ("DEPLOY", 1), ("20.30", 1)] {
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
        app.handle_event(Event::Paste(query.into()));
        assert_eq!(app.visible_profile_indices(), &[expected]);
        assert_eq!(app.selected_index(), Some(expected));
    }
    app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
    app.handle_event(Event::Paste("secret".into()));
    assert!(app.visible_profile_indices().is_empty());
}

#[test]
fn filtered_navigation_and_actions_use_actual_profile_indices() {
    let mut app = App::new(vec![
        profile("1", "dev"),
        profile("2", "prod api"),
        profile("3", "staging"),
        profile("4", "prod database"),
    ]);
    app.handle_event(Event::Paste("prod".into()));
    assert_eq!(app.visible_profile_indices(), &[1, 3]);
    assert_eq!(app.selected_index(), Some(1));
    app.handle_mouse_target(MouseTarget::Profile(0));
    assert_eq!(app.selected_index(), Some(1));
    app.handle_mouse_target(MouseTarget::Profile(3));
    assert_eq!(app.selected_index(), Some(3));
    app.handle_key(key(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(app.selected_index(), Some(1));
    app.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(app.selected_index(), Some(3));
    assert_eq!(
        app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE)),
        AppAction::Connect("4".into())
    );

    app.handle_key(key(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().profile_id.as_deref(), Some("4"));
    app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.browse_query(), "prod");
    assert_eq!(app.selected_index(), Some(3));

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(app.selected_index(), Some(1));
    app.handle_key(key(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert_eq!(
        app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE)),
        AppAction::Delete("2".into())
    );
    app.finish_delete("2");
    assert_eq!(app.browse_query(), "prod");
    assert_eq!(app.visible_profile_indices(), &[2]);
    assert_eq!(app.selected_profile().unwrap().id, "4");
}

#[test]
fn no_filter_results_disable_connection_edit_and_delete() {
    let mut app = App::new(vec![profile("1", "one")]);
    app.handle_event(Event::Paste("missing".into()));
    assert!(app.visible_profile_indices().is_empty());
    assert!(app.selected_profile().is_none());

    for event in [
        key(KeyCode::Enter, KeyModifiers::NONE),
        key(KeyCode::Down, KeyModifiers::NONE),
        key(KeyCode::Char('e'), KeyModifiers::CONTROL),
        key(KeyCode::Char('d'), KeyModifiers::CONTROL),
    ] {
        assert_eq!(app.handle_key(event), AppAction::None);
        assert_eq!(app.screen(), Screen::Browse);
        assert_eq!(app.selected_index(), None);
    }

    for target in [MouseTarget::Connect, MouseTarget::Edit, MouseTarget::Delete] {
        let mut regions = MouseRegions::default();
        regions.add(0, 0, 10, 1, target);
        app.set_mouse_regions(regions);
        assert_eq!(
            app.handle_mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 1,
                row: 0,
                modifiers: KeyModifiers::NONE,
            }),
            AppAction::None
        );
        assert_eq!(app.screen(), Screen::Browse);
    }
    app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.visible_profile_indices(), &[0]);
    assert_eq!(app.selected_index(), Some(0));
}

#[test]
fn search_backspace_removes_complete_graphemes_and_escape_clears() {
    let mut app = App::new(vec![profile("1", "生产👩‍💻")]);
    app.handle_key(key(KeyCode::Char('/'), KeyModifiers::NONE));
    assert_eq!(app.browse_query(), "");
    app.handle_key(key(KeyCode::Char('生'), KeyModifiers::NONE));
    app.handle_event(Event::Paste("产👩‍💻\r\n".into()));
    assert_eq!(app.browse_query(), "生产👩‍💻");
    app.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.browse_query(), "生产");
    app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.browse_query(), "");
    assert_eq!(app.screen(), Screen::Browse);
}

#[test]
fn canceled_add_keeps_filter_and_successful_save_reveals_saved_profile() {
    let mut app = App::new(vec![profile("1", "one")]);
    app.handle_event(Event::Paste("missing".into()));
    app.begin_add();
    app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.browse_query(), "missing");
    assert!(app.selected_profile().is_none());

    app.begin_add();
    app.finish_save(profile("2", "new"));
    assert_eq!(app.browse_query(), "");
    assert_eq!(app.visible_profile_indices(), &[0, 1]);
    assert_eq!(app.selected_profile().unwrap().id, "2");
}

#[test]
fn reloading_profiles_refreshes_search_matches() {
    let mut app = App::new(vec![profile("1", "one")]);
    app.handle_event(Event::Paste("prod".into()));
    app.set_profiles(vec![profile("2", "prod api"), profile("3", "dev")]);
    assert_eq!(app.visible_profile_indices(), &[0]);
    assert_eq!(app.selected_profile().unwrap().id, "2");
    app.set_profiles(vec![profile("3", "dev")]);
    assert!(app.selected_profile().is_none());
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
fn enter_saves_when_show_password_has_focus() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.name = "New".into();
    form.host = "example.com".into();
    form.username = "root".into();
    form.auth = AuthDraft::Password {
        password: "pw".into(),
    };
    form.field = FormField::ShowPassword;

    assert!(matches!(
        app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE)),
        AppAction::Save(_)
    ));
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

#[test]
fn ctrl_left_and_right_jump_words_in_form_fields() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.host = "prod-api-01.us-west.example.com".into();
    form.field = FormField::Host;
    form.cursor = form.host.chars().count();

    // Word boundaries stepping backward
    app.handle_key(key(KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().cursor, 28); // before "com"
    app.handle_key(key(KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().cursor, 20); // before "example"
    app.handle_key(key(KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().cursor, 15); // before "west"
    app.handle_key(key(KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().cursor, 12); // before "us"
    app.handle_key(key(KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().cursor, 9); // before "01"
    app.handle_key(key(KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().cursor, 5); // before "api"
    app.handle_key(key(KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().cursor, 0); // before "prod"

    // Stepping forward
    app.handle_key(key(KeyCode::Right, KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().cursor, 5);
    app.handle_key(key(KeyCode::Right, KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().cursor, 9);
}

#[test]
fn ctrl_w_and_ctrl_backspace_delete_words() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.name = "HongKong_BestAPI-4c4g-Jinyeyun-R".into();
    form.field = FormField::Name;
    form.cursor = form.name.chars().count();

    // Ctrl+W deletes previous word
    app.handle_key(key(KeyCode::Char('w'), KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().name, "HongKong_BestAPI-4c4g-Jinyeyun-");

    // Standard Ctrl+Backspace (sent as KeyCode::Backspace with CONTROL modifier)
    app.handle_key(key(KeyCode::Backspace, KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().name, "HongKong_BestAPI-4c4g-");

    // Terminal-emitted Ctrl+Backspace (sent as 0x08, parsed as Char('h') with CONTROL modifier in Konsole/xterm)
    app.handle_key(key(KeyCode::Char('h'), KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().name, "HongKong_BestAPI-");

    // Alt+Backspace
    app.handle_key(key(KeyCode::Backspace, KeyModifiers::ALT));
    assert_eq!(app.form().unwrap().name, "");
}

#[test]
fn ctrl_a_and_ctrl_e_jump_to_line_boundaries() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.host = "192.168.1.100".into();
    form.field = FormField::Host;
    form.cursor = 5;

    app.handle_key(key(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().cursor, 0);

    app.handle_key(key(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().cursor, 13);
}

#[test]
fn ctrl_u_and_ctrl_k_clear_line_segments() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.host = "prefix-target-suffix".into();
    form.field = FormField::Host;
    form.cursor = 13; // after "target"

    app.handle_key(key(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().host, "prefix-target");

    app.handle_key(key(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(app.form().unwrap().host, "");
    assert_eq!(app.form().unwrap().cursor, 0);
}

#[test]
fn new_connection_defaults_to_root_and_can_save_with_only_host() {
    let mut app = App::new(Vec::new());
    assert_eq!(app.form().unwrap().username, "root");
    assert_eq!(app.form().unwrap().port, "22");

    let form = app.form_mut().unwrap();
    form.host = "1.2.3.4".into();
    form.auth = AuthDraft::Password {
        password: "secret".into(),
    };

    let action = app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        action,
        AppAction::Save(ProfileDraft {
            username,
            port,
            host,
            ..
        }) if username == "root" && port == 22 && host == "1.2.3.4"
    ));
}

#[test]
fn empty_username_and_port_default_to_root_and_22() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.username.clear();
    form.port.clear();
    form.host = "1.2.3.4".into();
    form.auth = AuthDraft::Password {
        password: "secret".into(),
    };

    let action = app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        action,
        AppAction::Save(ProfileDraft {
            username,
            port,
            host,
            ..
        }) if username == "root" && port == 22 && host == "1.2.3.4"
    ));
}

#[test]
fn endpoint_shorthand_parses_ssh_and_port_flags() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.host = "ssh -p 2222 deploy@10.0.0.1".into();
    form.auth = AuthDraft::Password {
        password: "secret".into(),
    };

    let action = app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        action,
        AppAction::Save(ProfileDraft {
            username,
            port,
            host,
            ..
        }) if username == "deploy" && port == 2222 && host == "10.0.0.1"
    ));
}

#[test]
fn empty_import_path_tab_advances_without_error() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.auth = AuthDraft::Key {
        source: KeySource::Import,
        value: String::new(),
        public_key: None,
    };
    form.field = FormField::KeyValue;

    let action = app.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(action, AppAction::None);
    assert_eq!(app.form().unwrap().field, FormField::PublicKey);
    assert_eq!(app.form().unwrap().validation_error, None);
}

#[test]
fn port_field_ignores_non_digit_characters() {
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.port.clear();
    form.field = FormField::Port;
    form.cursor = 0;

    app.handle_key(key(KeyCode::Char('a'), KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Char('8'), KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Char('b'), KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Char('0'), KeyModifiers::NONE));

    assert_eq!(app.form().unwrap().port, "80");
}

#[test]
fn browse_navigation_supports_page_up_down_home_end_and_query_editing() {
    let profiles: Vec<_> = (0..20)
        .map(|i| profile(&i.to_string(), &format!("Server {i:02}")))
        .collect();
    let mut app = App::new(profiles);

    app.handle_key(key(KeyCode::End, KeyModifiers::NONE));
    assert_eq!(app.selected_index(), Some(19));

    app.handle_key(key(KeyCode::Home, KeyModifiers::NONE));
    assert_eq!(app.selected_index(), Some(0));

    app.handle_key(key(KeyCode::PageDown, KeyModifiers::NONE));
    assert_eq!(app.selected_index(), Some(5));

    app.handle_key(key(KeyCode::PageUp, KeyModifiers::NONE));
    assert_eq!(app.selected_index(), Some(0));

    app.handle_event(Event::Paste("my-test-query".into()));
    assert_eq!(app.browse_query(), "my-test-query");

    app.handle_key(key(KeyCode::Char('w'), KeyModifiers::CONTROL));
    assert_eq!(app.browse_query(), "my-test-");

    app.handle_key(key(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(app.browse_query(), "");
}
