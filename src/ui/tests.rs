use std::path::PathBuf;

use ratatui::{Terminal, backend::TestBackend};

use crate::{
    app::{App, AuthDraft, FormField, KeyChoice, KeySource, MouseTarget},
    config::{AuthMethod, Profile},
};

use super::{browse::visible_tabs, draw, form::existing_key_label, helpers::truncate};

fn profile(name: &str) -> Profile {
    let key_directory = PathBuf::from("keys").join(name);
    Profile {
        id: name.into(),
        name: name.into(),
        host: "example.com".into(),
        port: 22,
        username: "root".into(),
        auth: AuthMethod::Key {
            private_key: key_directory.join("key"),
            public_key: key_directory.join("key.pub"),
        },
    }
}

#[test]
fn renders_browse_and_each_modal_at_common_terminal_size() {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(vec![profile("Production"), profile("Staging")]);
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    app.begin_add();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    app.form_mut().unwrap().auth = AuthDraft::Key {
        source: KeySource::Paste,
        value: "-----BEGIN OPENSSH PRIVATE KEY-----".into(),
        public_key: None,
    };
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    app.handle_mouse_target(MouseTarget::Cancel);
    app.begin_delete();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    app.set_error("Permission denied (publickey,password)");
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
}

#[test]
fn selected_tab_remains_in_visible_range() {
    let profiles: Vec<_> = (0..20)
        .map(|index| profile(&format!("Connection {index}")))
        .collect();
    let (start, end) = visible_tabs(&profiles, 18, 40);
    assert!(start <= 18 && 18 < end);
    assert!(end <= profiles.len());
}

#[test]
fn truncation_respects_double_width_characters() {
    assert_eq!(truncate("连接到生产服务器", 7), "连接到…");
}

#[test]
fn path_label_resolves_shared_key() {
    let mut app = App::new(Vec::new());
    app.set_available_keys(vec![KeyChoice {
        label: "work-key".into(),
        private_key: PathBuf::from(
            "keys/.shared/key-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        public_key: PathBuf::from(
            "keys/.shared/key-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.pub",
        ),
        used_by: 2,
    }]);
    assert_eq!(
        existing_key_label(
            &app,
            "keys/.shared/key-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .as_deref(),
        Some("work-key  ·  used by 2 connections")
    );
}

#[test]
fn browse_card_shows_the_connection_key_directory() {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut profile = profile("Production");
    profile.auth = AuthMethod::Key {
        private_key: PathBuf::from("keys/Production/key"),
        public_key: PathBuf::from("keys/Production/key.pub"),
    };
    let mut app = App::new(vec![profile]);

    terminal.draw(|frame| draw(frame, &mut app)).unwrap();

    let buffer = terminal.backend().buffer();
    let rendered = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Production/key"));
}

#[test]
fn import_form_renders_private_and_optional_public_path_inputs() {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(vec![profile("Production")]);
    app.begin_add();
    app.form_mut().unwrap().auth = AuthDraft::Key {
        source: KeySource::Import,
        value: "/tmp/private-key".into(),
        public_key: Some("/tmp/public-key".into()),
    };

    terminal.draw(|frame| draw(frame, &mut app)).unwrap();

    let buffer = terminal.backend().buffer();
    let rendered = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Private key file to import"));
    assert!(rendered.contains("Public key file (optional"));
    assert!(rendered.contains("/tmp/public-key"));
}

#[test]
fn active_empty_key_path_cursor_is_inside_the_input_body() {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(vec![profile("Production")]);
    app.begin_add();
    let form = app.form_mut().unwrap();
    form.auth = AuthDraft::Key {
        source: KeySource::Import,
        value: String::new(),
        public_key: None,
    };
    form.field = crate::app::FormField::KeyValue;
    form.cursor = 0;

    terminal.draw(|frame| draw(frame, &mut app)).unwrap();

    let buffer = terminal.backend().buffer();
    let (expected_y, expected_x) = (0..buffer.area.height)
        .find_map(|y| {
            let row = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>();
            row.contains("Paste or type here").then(|| {
                let x = (0..buffer.area.width)
                    .find(|x| buffer[(*x, y)].symbol() == "P")
                    .expect("placeholder start cell");
                (y, x)
            })
        })
        .expect("private-key input placeholder should be rendered");
    let cursor = terminal.get_cursor_position().unwrap();
    assert_eq!(cursor.y, expected_y, "cursor must be on the input body row");
    assert_eq!(
        cursor.x, expected_x,
        "empty input cursor must start before the placeholder"
    );
}

#[test]
fn active_key_path_cursor_tracks_the_editing_position() {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(Vec::new());
    let form = app.form_mut().unwrap();
    form.auth = AuthDraft::Key {
        source: KeySource::Import,
        value: "/tmp/private-key".into(),
        public_key: None,
    };
    form.field = FormField::KeyValue;
    form.cursor = 4;

    terminal.draw(|frame| draw(frame, &mut app)).unwrap();

    let buffer = terminal.backend().buffer();
    let (value_y, value_x) = (0..buffer.area.height)
        .find_map(|y| {
            let row = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>();
            row.contains("/tmp/private-key").then(|| {
                let x = (0..buffer.area.width)
                    .find(|x| buffer[(*x, y)].symbol() == "/")
                    .unwrap();
                (y, x)
            })
        })
        .unwrap();
    let cursor = terminal.get_cursor_position().unwrap();
    assert_eq!((cursor.x, cursor.y), (value_x + 4, value_y));
}
