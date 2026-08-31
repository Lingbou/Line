use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

use crate::app::{App, MouseRegions, MouseTarget};

use super::{
    helpers::{ButtonTone, draw_button, inset, modal_rect},
    theme::{DANGER, MUTED, SURFACE, TEXT, WARNING},
};

pub(super) fn draw_delete_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    regions: &mut MouseRegions,
) {
    let modal = modal_rect(area, 60, 12);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DANGER))
            .title(Span::styled(
                "  Delete connection?  ",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(SURFACE)),
        modal,
    );
    let inner = inset(modal, 2, 1);
    let name = app
        .delete_target_profile()
        .map(|profile| profile.name.as_str())
        .unwrap_or("this connection");
    let message = Text::from(vec![
        Line::from(vec![
            Span::styled("Remove ", Style::default().fg(MUTED)),
            Span::styled(name, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled("?", Style::default().fg(MUTED)),
        ]),
        Line::raw(""),
        Line::styled(
            "The shared key files will be kept.",
            Style::default().fg(WARNING),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(message).wrap(Wrap { trim: true }),
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(4),
        ),
    );
    let button_area = Rect::new(inner.x, modal.bottom().saturating_sub(4), inner.width, 3);
    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(1),
            Constraint::Percentage(50),
        ])
        .split(button_area);
    draw_button(frame, buttons[0], "DELETE  Enter", ButtonTone::Danger);
    draw_button(frame, buttons[2], "CANCEL  Esc", ButtonTone::Quiet);
    regions.add(
        buttons[0].x,
        buttons[0].y,
        buttons[0].width,
        buttons[0].height,
        MouseTarget::Confirm,
    );
    regions.add(
        buttons[2].x,
        buttons[2].y,
        buttons[2].width,
        buttons[2].height,
        MouseTarget::Cancel,
    );
}

pub(super) fn draw_delete_key_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    regions: &mut MouseRegions,
) {
    let modal = modal_rect(area, 64, 12);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DANGER))
            .title(Span::styled(
                "  Delete imported key pair?  ",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(SURFACE)),
        modal,
    );
    let inner = inset(modal, 2, 1);
    let label = app
        .delete_key_target()
        .map(|key| key.label.as_str())
        .unwrap_or("this key");
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(vec![
                Span::styled("Permanently remove ", Style::default().fg(MUTED)),
                Span::styled(
                    label,
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Span::styled("?", Style::default().fg(MUTED)),
            ]),
            Line::raw(""),
            Line::styled(
                "Both private and public key files will be deleted.",
                Style::default().fg(WARNING),
            ),
        ]))
        .wrap(Wrap { trim: true }),
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(4),
        ),
    );
    let button_area = Rect::new(inner.x, modal.bottom().saturating_sub(4), inner.width, 3);
    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(1),
            Constraint::Percentage(50),
        ])
        .split(button_area);
    draw_button(frame, buttons[0], "DELETE  Enter", ButtonTone::Danger);
    draw_button(frame, buttons[2], "CANCEL  Esc", ButtonTone::Quiet);
    regions.add(
        buttons[0].x,
        buttons[0].y,
        buttons[0].width,
        buttons[0].height,
        MouseTarget::Confirm,
    );
    regions.add(
        buttons[2].x,
        buttons[2].y,
        buttons[2].width,
        buttons[2].height,
        MouseTarget::Cancel,
    );
}

pub(super) fn draw_error_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    regions: &mut MouseRegions,
) {
    let modal = modal_rect(area, 72, 15);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DANGER))
            .title(Span::styled(
                "  Something went wrong  ",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(SURFACE)),
        modal,
    );
    let inner = inset(modal, 2, 1);
    frame.render_widget(
        Paragraph::new(app.error_message().unwrap_or("Unknown error"))
            .wrap(Wrap { trim: false })
            .scroll((app.error_scroll(), 0))
            .style(Style::default().fg(TEXT)),
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(4),
        ),
    );
    let button = Rect::new(
        inner.x.saturating_add(inner.width / 4),
        modal.bottom().saturating_sub(4),
        inner.width / 2,
        3,
    );
    draw_button(frame, button, "OK  Enter", ButtonTone::Accent);
    regions.add(
        button.x,
        button.y,
        button.width,
        button.height,
        MouseTarget::Dismiss,
    );
}
