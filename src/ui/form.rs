use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::app::{
    App, AuthDraft, FormField, FormState, KeySource, MouseRegions, MouseTarget, SaveMode,
};

use super::{
    helpers::{ButtonTone, draw_button, input_window, inset, modal_rect},
    theme::{ACCENT, ACCENT_2, DANGER, MUTED, SURFACE, SURFACE_2, TEXT},
};

pub(super) fn draw_form_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    regions: &mut MouseRegions,
) {
    let Some(form) = app.form() else {
        return;
    };
    let desired_height = match &form.auth {
        AuthDraft::Password { .. } => 20,
        AuthDraft::Key {
            source: KeySource::Import | KeySource::Paste,
            ..
        } => 22,
        AuthDraft::Key { .. } => 21,
    };
    let modal = modal_rect(area, 88, desired_height);
    frame.render_widget(Clear, modal);
    let title = match form.mode {
        SaveMode::Add => "  New connection  ",
        SaveMode::Edit => "  Edit connection  ",
    };
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT_2))
            .title(Span::styled(
                title,
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(SURFACE)),
        modal,
    );
    let inner = inset(modal, 2, 1);
    let button_y = modal.bottom().saturating_sub(4);
    let content_bottom = button_y.saturating_sub(1);
    let mut y = inner.y;

    let identity = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Length(1),
            Constraint::Percentage(45),
        ])
        .split(Rect::new(inner.x, y, inner.width, 3));
    render_input(
        frame,
        identity[0],
        "Name",
        &form.name,
        form.field == FormField::Name,
        false,
        form.cursor,
        regions,
        FormField::Name,
    );
    render_input(
        frame,
        identity[2],
        "Username",
        &form.username,
        form.field == FormField::Username,
        false,
        form.cursor,
        regions,
        FormField::Username,
    );
    y = y.saturating_add(3);

    let host_port = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(12),
        ])
        .split(Rect::new(inner.x, y, inner.width, 3));
    render_input(
        frame,
        host_port[0],
        "Host / user@host:port",
        &form.host,
        form.field == FormField::Host,
        false,
        form.cursor,
        regions,
        FormField::Host,
    );
    render_input(
        frame,
        host_port[2],
        "Port",
        &form.port,
        form.field == FormField::Port,
        false,
        form.cursor,
        regions,
        FormField::Port,
    );
    y = y.saturating_add(3);

    let auth_area = Rect::new(inner.x, y, inner.width, 1);
    render_segmented_auth(frame, auth_area, form, regions);
    y = y.saturating_add(1);

    match &form.auth {
        AuthDraft::Password { password } => {
            if y.saturating_add(3) <= content_bottom {
                render_input(
                    frame,
                    Rect::new(inner.x, y, inner.width, 3),
                    if form.mode == SaveMode::Edit {
                        "Saved password (leave blank to keep current)"
                    } else {
                        "Saved password"
                    },
                    password,
                    form.field == FormField::Password,
                    !form.show_password,
                    form.cursor,
                    regions,
                    FormField::Password,
                );
                y = y.saturating_add(3);
            }
            if y < content_bottom {
                let rect = Rect::new(inner.x, y, inner.width, 1);
                let mark = if form.show_password { "[✓]" } else { "[ ]" };
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(
                            mark,
                            Style::default().fg(ACCENT).add_modifier(
                                if form.field == FormField::ShowPassword {
                                    Modifier::BOLD
                                } else {
                                    Modifier::empty()
                                },
                            ),
                        ),
                        Span::styled(
                            " Show password  (Space)",
                            Style::default().fg(if form.field == FormField::ShowPassword {
                                TEXT
                            } else {
                                MUTED
                            }),
                        ),
                    ]))
                    .style(Style::default().bg(
                        if form.field == FormField::ShowPassword {
                            SURFACE_2
                        } else {
                            SURFACE
                        },
                    )),
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
        }
        AuthDraft::Key {
            source,
            value,
            public_key,
        } => {
            if y < content_bottom {
                render_key_sources(
                    frame,
                    Rect::new(inner.x, y, inner.width, 1),
                    *source,
                    regions,
                );
                y = y.saturating_add(1);
            }
            let private_height = if *source == KeySource::Paste { 4 } else { 3 };
            if y.saturating_add(private_height) <= content_bottom {
                let (label, displayed) = match source {
                    KeySource::Import => (
                        "Private key file to import  (Tab to complete)",
                        value.clone(),
                    ),
                    KeySource::Existing => (
                        "Existing key  (←/→ to choose)",
                        existing_key_label(app, value).unwrap_or_else(|| {
                            if app.available_keys().is_empty() {
                                "No imported keys — choose Import or Paste".into()
                            } else {
                                value.clone()
                            }
                        }),
                    ),
                    KeySource::Paste => ("Private key (paste)", value.clone()),
                };
                render_text_box(
                    frame,
                    Rect::new(inner.x, y, inner.width, private_height),
                    label,
                    &displayed,
                    (form.field == FormField::KeyValue).then_some(form.cursor),
                    regions,
                    FormField::KeyValue,
                );
                y = y.saturating_add(private_height);
            }
            if *source == KeySource::Existing && y < content_bottom {
                let selected = app
                    .available_keys()
                    .iter()
                    .find(|key| key.private_key.to_string_lossy() == value.as_str());
                let (label, tone) = match selected {
                    Some(key) if key.used_by == 0 => {
                        ("DELETE UNUSED KEY  Ctrl+D", ButtonTone::Danger)
                    }
                    Some(key) => {
                        let label = if key.used_by == 1 {
                            "Key used by 1 connection"
                        } else {
                            "Key is shared — deletion disabled"
                        };
                        (label, ButtonTone::Quiet)
                    }
                    None => ("No existing key selected", ButtonTone::Quiet),
                };
                let rect = Rect::new(inner.x, y, inner.width.min(42), 1);
                draw_button(frame, rect, label, tone);
                if selected.is_some_and(|key| key.used_by == 0) {
                    regions.add(
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height,
                        MouseTarget::DeleteKey,
                    );
                }
                y = y.saturating_add(1);
            }
            match source {
                KeySource::Import if y.saturating_add(3) <= content_bottom => {
                    render_text_box(
                        frame,
                        Rect::new(inner.x, y, inner.width, 3),
                        "Public key file (optional — derived when blank; Tab to complete)",
                        public_key.as_deref().unwrap_or(""),
                        (form.field == FormField::PublicKey).then_some(form.cursor),
                        regions,
                        FormField::PublicKey,
                    );
                }
                KeySource::Paste if y.saturating_add(4) <= content_bottom => {
                    render_text_box(
                        frame,
                        Rect::new(inner.x, y, inner.width, 4),
                        "Public key (optional — generated when blank)",
                        public_key.as_deref().unwrap_or(""),
                        (form.field == FormField::PublicKey).then_some(form.cursor),
                        regions,
                        FormField::PublicKey,
                    );
                }
                _ => {}
            }
        }
    }

    if let Some(error) = &form.validation_error {
        let error_area = Rect::new(inner.x, button_y.saturating_sub(1), inner.width, 1);
        frame.render_widget(
            Paragraph::new(format!("! {error}"))
                .style(Style::default().fg(DANGER).add_modifier(Modifier::BOLD)),
            error_area,
        );
    }

    let button_area = Rect::new(
        inner.x,
        button_y,
        inner.width,
        3.min(modal.bottom() - button_y),
    );
    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(62),
            Constraint::Length(1),
            Constraint::Percentage(38),
        ])
        .split(button_area);
    draw_button(frame, buttons[0], "SAVE  ↵", ButtonTone::Accent);
    draw_button(frame, buttons[2], "CANCEL  Esc", ButtonTone::Quiet);
    regions.add(
        buttons[0].x,
        buttons[0].y,
        buttons[0].width,
        buttons[0].height,
        MouseTarget::Save,
    );
    regions.add(
        buttons[2].x,
        buttons[2].y,
        buttons[2].width,
        buttons[2].height,
        MouseTarget::Cancel,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_input(
    frame: &mut Frame<'_>,
    rect: Rect,
    title: &str,
    value: &str,
    active: bool,
    masked: bool,
    cursor: usize,
    regions: &mut MouseRegions,
    field: FormField,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let raw_displayed = if masked {
        "•".repeat(value.chars().count())
    } else {
        value.replace(['\n', '\r'], " ")
    };
    let available_width = rect.width.saturating_sub(2) as usize;
    let (displayed, visible_cursor) = input_window(&raw_displayed, cursor, available_width);
    let placeholder = match field {
        FormField::Name => "Connection 1",
        FormField::Host => "192.168.1.10",
        FormField::Port => "22",
        FormField::Username => "root",
        FormField::Password => "Password",
        FormField::ShowPassword => "",
        _ => "",
    };
    let line = if displayed.is_empty() {
        Line::styled(placeholder, Style::default().fg(MUTED))
    } else {
        Line::styled(displayed.as_str(), Style::default().fg(TEXT))
    };
    let block = input_block(title, active);
    frame.render_widget(Paragraph::new(line).block(block), rect);
    regions.add(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        MouseTarget::Field(field),
    );
    if active && rect.width > 2 && rect.height >= 3 {
        let prefix: String = displayed.chars().take(visible_cursor).collect();
        let offset = UnicodeWidthStr::width(prefix.as_str()) as u16;
        let x = rect
            .x
            .saturating_add(1)
            .saturating_add(offset.min(rect.width - 2));
        frame.set_cursor_position((x, rect.y.saturating_add(1)));
    }
}

fn render_text_box(
    frame: &mut Frame<'_>,
    rect: Rect,
    title: &str,
    value: &str,
    active_cursor: Option<usize>,
    regions: &mut MouseRegions,
    field: FormField,
) {
    let displayed = if value.is_empty() {
        "Paste or type here"
    } else {
        value
    };
    frame.render_widget(
        Paragraph::new(displayed)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(if value.is_empty() { MUTED } else { TEXT }))
            .block(input_block(title, active_cursor.is_some())),
        rect,
    );
    if let Some(cursor) = active_cursor
        && rect.width > 2
        && rect.height > 2
    {
        // Placeholder text is visual guidance, not input. Cursor coordinates
        // must be based on the real value and offset past the top/left border.
        let before_cursor = value.chars().take(cursor).collect::<String>();
        let lines: Vec<&str> = before_cursor.split('\n').collect();
        let last = lines.last().copied().unwrap_or("");
        let x = rect
            .x
            .saturating_add(1)
            .saturating_add(UnicodeWidthStr::width(last) as u16)
            .min(rect.right().saturating_sub(2));
        let y = rect
            .y
            .saturating_add(1)
            .saturating_add(lines.len().saturating_sub(1) as u16)
            .min(rect.bottom().saturating_sub(2));
        frame.set_cursor_position((x, y));
    }
    regions.add(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        MouseTarget::Field(field),
    );
}

fn input_block<'a>(title: &'a str, active: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if active { ACCENT } else { SURFACE_2 }))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(if active { ACCENT } else { MUTED }),
        ))
        .style(Style::default().bg(SURFACE_2))
}

fn render_segmented_auth(
    frame: &mut Frame<'_>,
    area: Rect,
    form: &FormState,
    regions: &mut MouseRegions,
) {
    let parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    let password = matches!(form.auth, AuthDraft::Password { .. });
    draw_segment(
        frame,
        parts[0],
        "●  PASSWORD",
        password,
        form.field == FormField::Authentication,
    );
    draw_segment(
        frame,
        parts[1],
        "◆  SSH KEY",
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
        parts[1].x,
        parts[1].y,
        parts[1].width,
        parts[1].height,
        MouseTarget::AuthKey,
    );
}

fn draw_segment(frame: &mut Frame<'_>, rect: Rect, label: &str, selected: bool, focused: bool) {
    let style = Style::default()
        .fg(if focused {
            ACCENT
        } else if selected {
            TEXT
        } else {
            MUTED
        })
        .bg(if selected { ACCENT_2 } else { SURFACE_2 })
        .add_modifier(if selected {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
    if rect.height < 3 {
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .style(style),
            rect,
        );
        return;
    }
    frame.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(style)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(if focused { ACCENT } else { SURFACE_2 })),
            ),
        rect,
    );
}

fn render_key_sources(
    frame: &mut Frame<'_>,
    area: Rect,
    selected: KeySource,
    regions: &mut MouseRegions,
) {
    let parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(area);
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
            parts[index],
            source.label(),
            source == selected,
            false,
        );
        regions.add(
            parts[index].x,
            parts[index].y,
            parts[index].width,
            parts[index].height,
            target,
        );
    }
}

pub(super) fn existing_key_label(app: &App, value: &str) -> Option<String> {
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
