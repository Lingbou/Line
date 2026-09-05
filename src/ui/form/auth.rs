use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::app::{
    App, AuthDraft, FormField, FormState, KeySource, MouseRegions, MouseTarget, SaveMode,
};

use super::{
    super::theme::{ACCENT, ACCENT_2, DANGER, MUTED, SURFACE, SURFACE_2, TEXT},
    fields::render_text_box,
};

pub(super) fn password_label(mode: SaveMode) -> &'static str {
    if mode == SaveMode::Edit {
        "Password · blank keeps saved"
    } else {
        "Password"
    }
}

pub(super) fn key_value(app: &App, source: KeySource, value: &str) -> (&'static str, String) {
    match source {
        KeySource::Import => ("Private key file", value.into()),
        KeySource::Existing => (
            "Saved key",
            existing_key_label(app, value).unwrap_or_else(|| {
                if app.available_keys().is_empty() {
                    "No keys — choose Import or Paste".into()
                } else {
                    value.into()
                }
            }),
        ),
        KeySource::Paste => ("Private key", value.into()),
    }
}

pub(super) fn render_public_key(
    frame: &mut Frame<'_>,
    rect: Rect,
    source: KeySource,
    value: &str,
    form: &FormState,
    regions: &mut MouseRegions,
) {
    let label = match source {
        KeySource::Import => "Public key file · optional",
        KeySource::Paste => "Public key · optional",
        KeySource::Existing => return,
    };
    render_text_box(
        frame,
        rect,
        label,
        value,
        form.field == FormField::PublicKey,
        (form.field == FormField::PublicKey).then_some(form.cursor),
        regions,
        FormField::PublicKey,
    );
}

pub(super) fn render_existing_key_action(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    value: &str,
    regions: &mut MouseRegions,
) {
    let selected = app
        .available_keys()
        .iter()
        .find(|key| key.private_key.to_string_lossy() == value);
    let (label, color) = match selected {
        Some(key) if key.used_by == 0 => ("Ctrl+D  Delete unused key", DANGER),
        Some(key) if key.used_by == 1 => ("Key used by 1 connection", MUTED),
        Some(_) => ("Shared key · already in use", MUTED),
        None => ("No existing key selected", MUTED),
    };
    let rect = Rect::new(
        area.x.saturating_add(2),
        area.y,
        area.width.saturating_sub(2),
        area.height,
    );
    frame.render_widget(
        Paragraph::new(label).style(Style::default().fg(color)),
        rect,
    );
    if selected.is_some_and(|key| key.used_by == 0) {
        regions.add(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            MouseTarget::DeleteKey,
        );
    }
}

pub(super) fn render_show_password(
    frame: &mut Frame<'_>,
    rect: Rect,
    form: &FormState,
    regions: &mut MouseRegions,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let rect = Rect::new(
        rect.x.saturating_add(2),
        rect.y,
        rect.width.saturating_sub(2),
        rect.height,
    );
    let active = form.field == FormField::ShowPassword;
    let mark = if form.show_password { "[×]" } else { "[ ]" };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                mark,
                Style::default().fg(ACCENT).add_modifier(if active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            ),
            Span::styled(
                " Show password",
                Style::default().fg(if active { TEXT } else { MUTED }),
            ),
            Span::styled("  Space", Style::default().fg(MUTED)),
        ]))
        .style(Style::default().bg(if active { SURFACE_2 } else { SURFACE })),
        rect,
    );
    regions.add(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        MouseTarget::ShowPassword,
    );
}

pub(super) fn render_segmented_auth(
    frame: &mut Frame<'_>,
    area: Rect,
    form: &FormState,
    regions: &mut MouseRegions,
) {
    let parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(13),
            Constraint::Length(1),
            Constraint::Length(12),
            Constraint::Min(0),
        ])
        .split(area);
    let password = matches!(form.auth, AuthDraft::Password { .. });
    draw_segment(
        frame,
        parts[0],
        "Password",
        password,
        form.field == FormField::Authentication,
    );
    draw_segment(
        frame,
        parts[2],
        "SSH key",
        !password,
        form.field == FormField::Authentication,
    );
    regions.add(
        parts[0].x,
        parts[0].y,
        parts[0].width,
        parts[0].height,
        MouseTarget::AuthPassword,
    );
    regions.add(
        parts[2].x,
        parts[2].y,
        parts[2].width,
        parts[2].height,
        MouseTarget::AuthKey,
    );
}

fn draw_segment(frame: &mut Frame<'_>, rect: Rect, label: &str, selected: bool, focused: bool) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let style = Style::default()
        .fg(if focused && selected {
            ACCENT
        } else if selected {
            TEXT
        } else {
            MUTED
        })
        .bg(if selected && focused {
            ACCENT_2
        } else if selected {
            SURFACE_2
        } else {
            SURFACE
        })
        .add_modifier(if selected {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
    let visual = Rect::new(rect.x, rect.y + rect.height / 2, rect.width, 1);
    frame.render_widget(Block::default().style(style), visual);
    frame.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(style),
        visual,
    );
}

pub(super) fn render_key_sources(
    frame: &mut Frame<'_>,
    area: Rect,
    selected: KeySource,
    focused: bool,
    regions: &mut MouseRegions,
) {
    let widths = [10, 11, 8];
    let parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(widths[0]),
            Constraint::Length(1),
            Constraint::Length(widths[1]),
            Constraint::Length(1),
            Constraint::Length(widths[2]),
            Constraint::Min(0),
        ])
        .split(area);
    let labels = ["Import", "Existing", "Paste"];
    for (index, (source, target)) in [
        (KeySource::Import, MouseTarget::KeyImport),
        (KeySource::Existing, MouseTarget::KeyExisting),
        (KeySource::Paste, MouseTarget::KeyPaste),
    ]
    .into_iter()
    .enumerate()
    {
        draw_segment(
            frame,
            parts[index * 2],
            labels[index],
            source == selected,
            focused,
        );
        regions.add(
            parts[index * 2].x,
            parts[index * 2].y,
            parts[index * 2].width,
            parts[index * 2].height,
            target,
        );
    }
}

pub(crate) fn existing_key_label(app: &App, value: &str) -> Option<String> {
    app.available_keys().iter().find_map(|key| {
        (key.private_key.to_string_lossy() == value).then(|| {
            format!(
                "{}  ·  used by {} connection{}",
                key.label,
                key.used_by,
                if key.used_by == 1 { "" } else { "s" }
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn key_source_focus_is_independent_from_the_selected_source() {
        let unfocused = render_sources(KeySource::Existing, false);
        let focused = render_sources(KeySource::Existing, true);

        let unfocused_import = &unfocused.backend().buffer()[(4, 0)];
        let unfocused_existing = &unfocused.backend().buffer()[(16, 0)];
        let focused_import = &focused.backend().buffer()[(4, 0)];
        let focused_existing = &focused.backend().buffer()[(16, 0)];

        assert_eq!(unfocused_import.fg, MUTED);
        assert_eq!(unfocused_import.bg, SURFACE);
        assert_eq!(unfocused_existing.fg, TEXT);
        assert_eq!(unfocused_existing.bg, SURFACE_2);
        assert!(unfocused_existing.modifier.contains(Modifier::BOLD));

        assert_eq!(focused_import.fg, MUTED);
        assert_eq!(focused_import.bg, SURFACE);
        assert_eq!(focused_existing.fg, ACCENT);
        assert_eq!(focused_existing.bg, ACCENT_2);
        assert!(focused_existing.modifier.contains(Modifier::BOLD));
    }

    fn render_sources(selected: KeySource, focused: bool) -> Terminal<TestBackend> {
        let backend = TestBackend::new(45, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut regions = MouseRegions::default();
        terminal
            .draw(|frame| {
                render_key_sources(
                    frame,
                    Rect::new(0, 0, 45, 1),
                    selected,
                    focused,
                    &mut regions,
                );
            })
            .unwrap();
        terminal
    }
}
