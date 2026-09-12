use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{Terminal, backend::TestBackend};

use crate::{
    app::{App, AppAction, AuthDraft, FormField, KeyChoice, KeySource, MouseTarget, Screen},
    config::{AuthMethod, Profile},
};

use super::{browse::visible_profile_range, draw, form::existing_key_label, helpers::truncate};

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
fn layout_regression_shortcuts_are_readable_at_every_supported_width() {
    for (width, height) in [
        (40, 12),
        (40, 22),
        (55, 16),
        (64, 22),
        (69, 18),
        (70, 18),
        (72, 24),
        (80, 24),
        (120, 34),
    ] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut app = App::new(vec![profile("Production")]);
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let text = rendered_text(&terminal);
        for shortcut in ["Ctrl+T", "Ctrl+E", "Ctrl+D", "Ctrl+C"] {
            assert!(
                text.contains(shortcut),
                "{width}x{height} is missing {shortcut}"
            );
        }
        assert!(
            !text.contains('^'),
            "caret shortcut notation must not return"
        );
    }
}

#[test]
fn layout_regression_shortcut_labels_keep_their_mouse_targets_after_reflow() {
    for (width, height) in [(40, 12), (64, 22), (70, 22), (80, 24), (120, 34)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut app = App::new(vec![profile("Production")]);
        for (label, screen) in [("Ctrl+E", Screen::Form), ("Ctrl+D", Screen::ConfirmDelete)] {
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let (column, row) = rendered_position(&terminal, label).unwrap();
            app.handle_mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                modifiers: KeyModifiers::NONE,
            });
            assert_eq!(
                app.screen(),
                screen,
                "{width}x{height}: {label} click missed"
            );
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        }
    }
}

#[test]
fn layout_regression_mixed_width_names_do_not_move_destination_or_auth_columns() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let mut first = profile("Production");
    first.host = "first.example.com".into();
    let mut second = profile("家庭服务器 e\u{301} 👩‍💻");
    second.host = "second.example.com".into();
    let mut app = App::new(vec![first, second]);
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let (first_x, first_y) = rendered_position(&terminal, "root@first.example.com:22").unwrap();
    let (second_x, second_y) = rendered_position(&terminal, "root@second.example.com:22").unwrap();
    assert_eq!(
        first_x, second_x,
        "mixed-width names must not shift destination cells"
    );
    assert_eq!(first_y + 1, second_y);
    let buffer = terminal.backend().buffer();
    let auth_x = (0..80)
        .find(|&x| buffer[(x, first_y)].symbol() == "K")
        .unwrap();
    assert_eq!(buffer[(auth_x, second_y)].symbol(), "K");
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: second_x,
        row: second_y,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(app.selected_index(), Some(1));
}

#[test]
fn layout_regression_long_names_render_fully_and_auth_is_adjacent() {
    for (width, height) in [(100, 32), (120, 34)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let long_name = "production-cluster-node-ap-east-01";
        let mut p = profile(long_name);
        p.host = "198.51.100.76".into();
        p.port = 2222;
        let mut app = App::new(vec![p]);
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let text = rendered_text(&terminal);
        assert!(
            text.contains(long_name),
            "{width}x{height} should show the full connection name without truncation"
        );
        let (name_x, name_y) = rendered_position(&terminal, long_name).unwrap();
        let (auth_x, auth_y) = rendered_position(&terminal, "KEY").unwrap();
        let (dest_x, dest_y) = rendered_position(&terminal, "root@198.51.100.76:2222").unwrap();
        assert_eq!(name_y, auth_y);
        assert_eq!(auth_y, dest_y);
        assert!(
            auth_x > name_x,
            "auth column must follow the connection name"
        );
        assert!(
            dest_x > auth_x,
            "destination column must follow auth column"
        );
        assert!(
            auth_x.saturating_sub(name_x) <= 45,
            "name to auth distance should remain tightly proportioned"
        );
        assert!(
            dest_x.saturating_sub(auth_x) <= 8,
            "destination should immediately follow auth without a giant gap"
        );
    }
}

#[test]
fn layout_regression_host_uses_available_width_and_labels_share_a_grid() {
    for (width, height) in [(80, 24), (120, 34)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut app = App::new(Vec::new());
        let form = app.form_mut().unwrap();
        form.host = "production.ap-southeast.example.com".into();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(
            rendered_text(&terminal).contains("production.ap-southeast.example.com"),
            "{width}x{height} should not truncate a host while other fields have spare space"
        );
        let (name_x, _) = rendered_position(&terminal, "Name").unwrap();
        let (auth_x, _) = rendered_position(&terminal, "Authentication").unwrap();
        assert_eq!(
            name_x, auth_x,
            "field and section labels must share a left edge"
        );
    }
}

#[test]
fn layout_regression_wide_list_does_not_spend_every_other_row_on_a_gap() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let mut app = App::new(
        (0..12)
            .map(|i| profile(&format!("Server {i:02}")))
            .collect(),
    );
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    assert!(
        rendered_text(&terminal).contains("Server 09"),
        "a 24-row terminal should show at least ten compact connection rows"
    );
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
fn selected_profile_remains_in_visible_list_range() {
    let (start, end) = visible_profile_range(20, 18, 5);
    assert!(start <= 18 && 18 < end);
    assert!(end <= 20);
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
fn browse_detail_shows_the_connection_key_directory() {
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
fn responsive_layouts_render_at_supported_terminal_sizes() {
    for (width, height) in [(100, 32), (72, 24), (60, 24), (40, 22)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(vec![
            profile("Production"),
            profile("Staging"),
            profile("家庭服务器"),
        ]);

        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let browse = rendered_text(&terminal);
        assert!(browse.contains("Production"));
        assert!(browse.contains("root@example.com:22"));
        assert!(browse.contains("Connect"));

        app.begin_edit();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let form = rendered_text(&terminal);
        assert!(form.contains("Edit connection"));
        assert!(form.contains("Username"));
        assert!(form.contains("Host"));
        assert!(form.contains("SSH key"));
        assert!(form.contains("Enter  Save"));

        app.handle_mouse_target(MouseTarget::Cancel);
        app.begin_delete();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(rendered_text(&terminal).contains("Delete connection"));

        app.set_error("Permission denied (publickey,password)");
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let error = rendered_text(&terminal);
        assert!(error.contains("Permission denied"));
        assert!(error.contains("Close"));
    }
}

#[test]
fn minimum_supported_layouts_keep_primary_content_visible() {
    let backend = TestBackend::new(40, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(vec![profile("Production"), profile("Staging")]);
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let browse = rendered_text(&terminal);
    assert!(browse.contains("Production"));
    assert!(browse.contains("KEY"));
    assert!(browse.contains("Connect"));

    let backend = TestBackend::new(40, 22);
    let mut terminal = Terminal::new(backend).unwrap();
    app.begin_add();
    app.form_mut().unwrap().auth = AuthDraft::Key {
        source: KeySource::Import,
        value: "/tmp/private".into(),
        public_key: Some("/tmp/public".into()),
    };
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let form = rendered_text(&terminal);
    assert!(form.contains("Private key file"));
    assert!(form.contains("Public key file · optional"));
    assert!(form.contains("/tmp/private"));
    assert!(form.contains("/tmp/public"));
    assert!(form.contains("Enter  Save"));
}

#[test]
fn empty_browse_uses_one_centered_call_to_action() {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(vec![profile("Production")]);
    app.finish_delete("Production");

    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let rendered = rendered_text(&terminal);
    assert!(rendered.contains("No connections yet"));
    assert_eq!(rendered.matches("New connection").count(), 1);

    let (column, row) = rendered_position(&terminal, "New connection").unwrap();
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(app.screen(), Screen::Form);
}

#[test]
fn connection_list_and_primary_action_have_working_mouse_regions() {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(vec![profile("Production"), profile("Staging")]);
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();

    let (column, row) = rendered_position(&terminal, "Staging").unwrap();
    assert_eq!(
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }),
        AppAction::None
    );
    assert_eq!(app.selected_profile().unwrap().name, "Staging");

    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let (column, row) = rendered_position(&terminal, "Enter  Connect").unwrap();
    assert_eq!(
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }),
        AppAction::Connect("Staging".into())
    );
}

#[test]
fn minimum_browse_layout_keeps_connect_mouse_action_available() {
    let backend = TestBackend::new(40, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(vec![profile("Production")]);
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();

    let (column, row) = rendered_position(&terminal, "Enter  Connect")
        .expect("minimum browse layout should render the primary Connect action");
    assert_eq!(
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }),
        AppAction::Connect("Production".into())
    );
}

#[test]
fn filtered_rows_connect_the_profile_clicked_after_scrolling() {
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    let mut app = App::new(
        (0..24)
            .map(|i| profile(&format!("Server {i:02}")))
            .collect(),
    );
    app.handle_event(Event::Paste("Server 1".into()));
    for _ in 0..8 {
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let (column, row) = rendered_position(&terminal, "Server 17")
        .expect("scrolling should keep nearby filtered rows visible");
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let (column, row) = rendered_position(&terminal, "Enter  Connect").unwrap();
    assert_eq!(
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }),
        AppAction::Connect("Server 17".into())
    );
}

#[test]
fn empty_filter_removes_old_mouse_actions_until_escape_restores_results() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let mut app = App::new(vec![profile("Production")]);
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let (column, row) = rendered_position(&terminal, "Enter  Connect").unwrap();
    app.handle_event(Event::Paste("missing host".into()));
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    assert!(rendered_text(&terminal).contains("No matching connections"));
    assert_eq!(
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }),
        AppAction::None
    );
    assert_eq!(app.screen(), Screen::Browse);
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    assert!(rendered_text(&terminal).contains("Production"));
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        AppAction::Connect("Production".into())
    );
}

#[test]
fn minimum_error_layout_allows_long_diagnostics_to_scroll() {
    let backend = TestBackend::new(40, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(vec![profile("Production")]);
    app.begin_edit();
    app.set_error("diagnostic ".repeat(24));
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();

    let rendered = rendered_text(&terminal);
    assert!(rendered.contains("Error"));
    assert!(rendered.contains("Close"));

    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));

    assert!(
        app.error_scroll() > 0,
        "PageDown should scroll a diagnostic that exceeds the visible error body"
    );
}

#[test]
fn error_modal_blocks_clicks_on_the_form_beneath_it() {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(vec![profile("Production")]);
    app.begin_add();
    app.set_error("Could not save the connection");
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();

    let (column, row) = rendered_position(&terminal, "Esc  Cancel")
        .expect("the dimmed form Cancel action should remain visible outside the error modal");
    assert_eq!(
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }),
        AppAction::None
    );
    assert_eq!(
        app.screen(),
        Screen::Error,
        "clicking a dimmed form action must not dismiss the error modal"
    );
}

#[test]
fn form_backed_error_does_not_leak_the_input_cursor_through_the_modal() {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(vec![profile("Production")]);
    app.begin_add();
    app.set_error("Could not save the connection");

    terminal.draw(|frame| draw(frame, &mut app)).unwrap();

    let cursor = terminal.get_cursor_position().unwrap();
    assert_eq!(
        (cursor.x, cursor.y),
        (0, 0),
        "a modal frame must not position the cursor in the form beneath it"
    );
}

#[test]
fn terminal_too_small_blocks_hidden_primary_and_destructive_actions() {
    let backend = TestBackend::new(39, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(vec![profile("Production"), profile("Staging")]);

    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        AppAction::None,
        "Enter must not connect while the browse controls are hidden"
    );
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        app.selected_profile().unwrap().name,
        "Production",
        "hidden connection selection must not react to the mouse wheel"
    );

    app.begin_delete();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        AppAction::None,
        "Enter must not confirm a deletion while its dialog is hidden"
    );
    assert_eq!(app.screen(), Screen::ConfirmDelete);
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.screen(), Screen::Browse, "Escape must remain available");

    app.begin_edit();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        AppAction::None,
        "Enter must not save while the form is hidden"
    );
    assert_eq!(app.screen(), Screen::Form);
}

fn rendered_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rendered_position(terminal: &Terminal<TestBackend>, needle: &str) -> Option<(u16, u16)> {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height).find_map(|y| {
        // Cell coordinates, not UTF-8 byte offsets: names can contain CJK,
        // combining marks and emoji before an ASCII destination/action.
        (0..buffer.area.width).find_map(|x| {
            let row = (x..buffer.area.width)
                .map(|column| buffer[(column, y)].symbol())
                .collect::<String>();
            row.starts_with(needle).then_some((x, y))
        })
    })
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
    assert!(rendered.contains("Private key file"));
    assert!(rendered.contains("Public key file · optional"));
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
