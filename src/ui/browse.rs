use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{App, AuthDraft, FormField, KeySource, MouseRegions, MouseTarget, Screen},
    config::{AuthMethod, Profile},
};

use super::{
    helpers::{ButtonTone, centered_fixed, draw_button, inset, truncate},
    theme::{ACCENT, BG, MUTED, SUCCESS, SURFACE, SURFACE_2, TEXT},
};

pub(super) fn draw_browse(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    regions: &mut MouseRegions,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(area);
    draw_header(frame, chunks[0], app);
    draw_tabs(frame, chunks[1], app, regions);
    draw_detail(frame, chunks[2], app, regions);
    draw_footer(frame, chunks[3], app);
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let count = app.profiles().len();
    let line = Line::from(vec![
        Span::styled(
            "  LINE",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  /  ", Style::default().fg(SURFACE_2)),
        Span::styled("SSH CONNECTIONS", Style::default().fg(TEXT)),
        Span::styled(
            format!(
                "    {count} {}",
                if count == 1 { "profile" } else { "profiles" }
            ),
            Style::default().fg(MUTED),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(line)
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(SURFACE_2)),
            )
            .alignment(Alignment::Left)
            .style(Style::default().bg(BG)),
        area,
    );
}

fn draw_tabs(frame: &mut Frame<'_>, area: Rect, app: &App, regions: &mut MouseRegions) {
    if area.width < 4 || area.height == 0 {
        return;
    }
    let inner = inset(area, 1, 0);
    let add_width = 5.min(inner.width);
    let tabs_width = inner.width.saturating_sub(add_width + 1);
    let selected = app.selected_index().unwrap_or(0);
    let (start, end) = visible_tabs(app.profiles(), selected, tabs_width);
    let left_hidden = start > 0;
    let right_hidden = end < app.profiles().len();
    let mut x = inner.x;

    if left_hidden && x < inner.right() {
        let rect = Rect::new(x, inner.y, 2.min(inner.right() - x), inner.height);
        frame.render_widget(
            Paragraph::new("‹")
                .alignment(Alignment::Center)
                .style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD)),
            rect,
        );
        x = x.saturating_add(rect.width);
    }

    for index in start..end {
        let profile = &app.profiles()[index];
        let desired = tab_width(&profile.name);
        let boundary = inner.x.saturating_add(tabs_width);
        if x >= boundary {
            break;
        }
        let width = desired.min(boundary - x);
        if width < 4 {
            break;
        }
        let rect = Rect::new(x, inner.y, width, inner.height);
        let active = index == selected;
        let label_width = width.saturating_sub(4) as usize;
        let label = truncate(&profile.name, label_width);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if active { ACCENT } else { SURFACE_2 }))
            .style(Style::default().bg(if active { SURFACE_2 } else { BG }));
        frame.render_widget(
            Paragraph::new(label)
                .block(block)
                .alignment(Alignment::Center)
                .style(
                    Style::default()
                        .fg(if active { TEXT } else { MUTED })
                        .add_modifier(if active {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            rect,
        );
        regions.add(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            MouseTarget::Tab(index),
        );
        x = x.saturating_add(width + 1);
    }

    if right_hidden {
        let boundary = inner.x.saturating_add(tabs_width);
        if boundary >= 2 {
            let rect = Rect::new(boundary - 2, inner.y, 2, inner.height);
            frame.render_widget(
                Paragraph::new("›")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD)),
                rect,
            );
        }
    }

    let add = Rect::new(
        inner.right().saturating_sub(add_width),
        inner.y,
        add_width,
        inner.height,
    );
    draw_button(frame, add, "+", ButtonTone::Accent);
    regions.add(add.x, add.y, add.width, add.height, MouseTarget::Add);
}

pub(super) fn visible_tabs(
    profiles: &[Profile],
    selected: usize,
    max_width: u16,
) -> (usize, usize) {
    if profiles.is_empty() || max_width < 4 {
        return (0, 0);
    }
    let selected = selected.min(profiles.len() - 1);
    let mut start = selected;
    let mut end = selected + 1;
    let mut used = tab_width(&profiles[selected].name);
    let mut take_left = true;
    loop {
        let candidate = if take_left && start > 0 {
            Some((start - 1, true))
        } else if end < profiles.len() {
            Some((end, false))
        } else if start > 0 {
            Some((start - 1, true))
        } else {
            None
        };
        let Some((index, is_left)) = candidate else {
            break;
        };
        let width = tab_width(&profiles[index].name).saturating_add(1);
        if used.saturating_add(width) > max_width {
            // A candidate from one side not fitting cannot rule out the other
            // side, so try it once before ending.
            if take_left && end < profiles.len() {
                take_left = false;
                continue;
            }
            break;
        }
        used = used.saturating_add(width);
        if is_left {
            start -= 1;
        } else {
            end += 1;
        }
        take_left = !take_left;
    }
    (start, end)
}

fn tab_width(label: &str) -> u16 {
    (UnicodeWidthStr::width(label) as u16 + 4).clamp(10, 28)
}

fn draw_detail(frame: &mut Frame<'_>, area: Rect, app: &App, regions: &mut MouseRegions) {
    if app.profiles().is_empty() {
        draw_empty_state(frame, area, regions);
        return;
    }
    let Some(profile) = app.selected_profile() else {
        return;
    };
    let card_width = area.width.saturating_sub(4).clamp(1, 84);
    let card_height = area.height.saturating_sub(2).clamp(1, 17);
    let card = centered_fixed(area, card_width, card_height);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE_2))
        .title(Line::from(vec![
            Span::styled("  ● ", Style::default().fg(SUCCESS)),
            Span::styled(
                truncate(&profile.name, card.width.saturating_sub(12) as usize),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
        ]))
        .style(Style::default().bg(SURFACE));
    frame.render_widget(block, card);
    let inner = inset(card, 3, 2);
    if inner.height == 0 {
        return;
    }

    let destination = format!("{}@{}", profile.username, profile.host);
    let auth = match &profile.auth {
        AuthMethod::Password { .. } => ("Password", "Saved locally".to_owned()),
        AuthMethod::Key { private_key, .. } => (
            "SSH key",
            private_key
                .strip_prefix("keys")
                .unwrap_or(private_key)
                .display()
                .to_string(),
        ),
    };
    let port = profile.port.to_string();
    let content = vec![
        detail_line("DESTINATION", &destination, ACCENT),
        Line::raw(""),
        detail_line("PORT", &port, TEXT),
        Line::raw(""),
        detail_line("AUTHENTICATION", auth.0, TEXT),
        Line::from(vec![
            Span::styled("                  ", Style::default()),
            Span::styled(auth.1, Style::default().fg(MUTED)),
        ]),
    ];
    let buttons_y = card.bottom().saturating_sub(4);
    let content_height = buttons_y.saturating_sub(inner.y);
    frame.render_widget(
        Paragraph::new(content)
            .style(Style::default().fg(TEXT).bg(SURFACE))
            .wrap(Wrap { trim: true }),
        Rect::new(inner.x, inner.y, inner.width, content_height),
    );

    let button_area = Rect::new(
        inner.x,
        buttons_y,
        inner.width,
        3.min(card.bottom() - buttons_y),
    );
    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(45),
            Constraint::Length(1),
            Constraint::Percentage(27),
            Constraint::Length(1),
            Constraint::Percentage(28),
        ])
        .split(button_area);
    draw_button(frame, buttons[0], "↵  CONNECT", ButtonTone::Accent);
    draw_button(frame, buttons[2], "EDIT", ButtonTone::Quiet);
    draw_button(frame, buttons[4], "DELETE", ButtonTone::Danger);
    for (rect, target) in [
        (buttons[0], MouseTarget::Connect),
        (buttons[2], MouseTarget::Edit),
        (buttons[4], MouseTarget::Delete),
    ] {
        regions.add(rect.x, rect.y, rect.width, rect.height, target);
    }
}

fn detail_line<'a>(label: &'a str, value: &'a str, color: Color) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label:<16}"), Style::default().fg(MUTED)),
        Span::styled(
            value,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn draw_empty_state(frame: &mut Frame<'_>, area: Rect, regions: &mut MouseRegions) {
    let card = centered_fixed(
        area,
        area.width.saturating_sub(4).clamp(1, 66),
        area.height.saturating_sub(2).clamp(1, 13),
    );
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::raw(""),
            Line::styled(
                "No connections yet",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Line::raw(""),
            Line::styled(
                "Add your first server and connect without retyping credentials.",
                Style::default().fg(MUTED),
            ),
        ]))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(SURFACE_2))
                .style(Style::default().bg(SURFACE)),
        ),
        card,
    );
    if card.height >= 4 {
        let button = Rect::new(
            card.x.saturating_add(card.width / 4),
            card.bottom().saturating_sub(4),
            card.width / 2,
            3,
        );
        draw_button(frame, button, "+  ADD CONNECTION", ButtonTone::Accent);
        regions.add(
            button.x,
            button.y,
            button.width,
            button.height,
            MouseTarget::Add,
        );
    }
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut spans = Vec::new();
    if let Some(status) = app.status_message() {
        spans.push(Span::styled(
            format!("  {status}  "),
            Style::default().fg(SUCCESS),
        ));
        spans.push(Span::styled("│  ", Style::default().fg(SURFACE_2)));
    }
    let shortcuts: Vec<(&str, &str)> = match app.screen() {
        Screen::Browse => vec![
            ("← →", "Select"),
            ("Enter", "Connect"),
            ("Ctrl+T", "New"),
            ("Ctrl+E", "Edit"),
            ("Ctrl+D", "Delete"),
            ("Ctrl+C", "Quit"),
        ],
        Screen::Form => {
            let mut shortcuts = vec![
                ("Tab", "Next"),
                ("Shift+Tab", "Back"),
                ("Enter", "Save"),
                ("Esc", "Cancel"),
            ];
            if matches!(
                app.form().map(|form| (&form.auth, form.field)),
                Some((
                    AuthDraft::Key {
                        source: KeySource::Existing,
                        ..
                    },
                    FormField::KeyValue
                ))
            ) {
                shortcuts.push(("Ctrl+D", "Delete unused key"));
            }
            shortcuts
        }
        Screen::ConfirmDelete | Screen::ConfirmDeleteKey => {
            vec![("Enter", "Confirm"), ("Esc", "Cancel")]
        }
        Screen::Error => vec![("↑ ↓", "Scroll"), ("Enter", "Close")],
    };
    for (key, label) in shortcuts {
        spans.push(Span::styled(
            format!(" {key} "),
            Style::default()
                .fg(ACCENT)
                .bg(SURFACE_2)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {label}  "),
            Style::default().fg(MUTED),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(SURFACE_2)),
            )
            .style(Style::default().bg(BG)),
        area,
    );
}
