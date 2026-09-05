use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

use crate::app::{App, MouseRegions, MouseTarget};

use super::{
    helpers::{ButtonTone, draw_button, inset, modal_rect, truncate},
    theme::{BORDER, MUTED, SURFACE, TEXT},
};

pub(super) fn draw_delete_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    regions: &mut MouseRegions,
) {
    let name = app
        .delete_target_profile()
        .map(|profile| profile.name.as_str())
        .unwrap_or("this connection");
    draw_confirmation(
        frame,
        area,
        "Delete connection",
        name,
        "The saved connection will be removed. Shared key files stay on disk.",
        regions,
    );
}

pub(super) fn draw_delete_key_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    regions: &mut MouseRegions,
) {
    let label = app
        .delete_key_target()
        .map(|key| key.label.as_str())
        .unwrap_or("this key");
    draw_confirmation(
        frame,
        area,
        "Delete imported key pair",
        label,
        "Both the private key and public key files will be permanently deleted.",
        regions,
    );
}

fn draw_confirmation(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    subject: &str,
    detail: &str,
    regions: &mut MouseRegions,
) {
    let modal = modal_rect(area, 62, 10);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .title(Span::styled(
                format!(" {title} "),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(SURFACE)),
        modal,
    );
    let inner = inset(modal, 2, 1);
    let actions_y = inner.bottom().saturating_sub(1);
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(vec![
                Span::styled("Remove ", Style::default().fg(MUTED)),
                Span::styled(
                    format!(
                        "“{}”",
                        truncate(subject, inner.width.saturating_sub(9) as usize)
                    ),
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Span::styled("?", Style::default().fg(MUTED)),
            ]),
            Line::raw(""),
            Line::styled(detail, Style::default().fg(MUTED)),
        ]))
        .wrap(Wrap { trim: true }),
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            actions_y.saturating_sub(inner.y).saturating_sub(1),
        ),
    );
    let button_area = Rect::new(inner.x, actions_y, inner.width, 1);
    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(13),
            Constraint::Length(1),
            Constraint::Length(15),
        ])
        .split(button_area);
    draw_button(frame, buttons[1], "Esc  Cancel", ButtonTone::Quiet);
    draw_button(frame, buttons[3], "Enter  Delete", ButtonTone::Danger);
    regions.add(
        buttons[3].x,
        buttons[3].y,
        buttons[3].width,
        buttons[3].height,
        MouseTarget::Confirm,
    );
    regions.add(
        buttons[1].x,
        buttons[1].y,
        buttons[1].width,
        buttons[1].height,
        MouseTarget::Cancel,
    );
}

pub(super) fn draw_error_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    regions: &mut MouseRegions,
) {
    let message = app.error_message().unwrap_or("Unknown error").to_owned();
    let body = Paragraph::new(message)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(TEXT));
    let width = 66.min(area.width.saturating_sub(2));
    let line_count = body.line_count(width.saturating_sub(4));
    let height = (line_count.saturating_add(6)).clamp(8, 14) as u16;
    let modal = modal_rect(area, width, height);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .title(Span::styled(
                " Error ",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(SURFACE)),
        modal,
    );
    let inner = inset(modal, 2, 1);
    let body_height = inner.height.saturating_sub(3);
    app.set_error_scroll_limit(
        line_count
            .saturating_sub(body_height as usize)
            .min(u16::MAX as usize) as u16,
    );
    frame.render_widget(
        body.scroll((app.error_scroll(), 0)),
        Rect::new(inner.x, inner.y, inner.width, body_height),
    );
    if body_height > 0 && line_count > body_height as usize {
        frame.render_widget(
            Paragraph::new("↑↓ scroll")
                .alignment(Alignment::Right)
                .style(Style::default().fg(MUTED)),
            Rect::new(inner.x, inner.y + body_height, inner.width, 1),
        );
    }
    let button_width = 14.min(inner.width);
    let button = Rect::new(
        inner.right().saturating_sub(button_width),
        inner.bottom().saturating_sub(1),
        button_width,
        1,
    );
    draw_button(frame, button, "Enter  Close", ButtonTone::Accent);
    regions.add(
        button.x,
        button.y,
        button.width,
        button.height,
        MouseTarget::Dismiss,
    );
}
