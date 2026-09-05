use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{App, MouseRegions, MouseTarget, Screen},
    ui::{
        helpers::{ButtonTone, draw_button, input_window},
        theme::{ACCENT, MUTED, SURFACE_2, TEXT},
    },
};

pub(super) fn draw_header(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    regions: &mut MouseRegions,
) {
    let new_width = if app.profiles().is_empty() {
        0
    } else if area.width >= 60 {
        24
    } else {
        13
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "line",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  /  connections", Style::default().fg(MUTED)),
        ])),
        Rect::new(area.x, area.y, area.width.saturating_sub(new_width), 1),
    );
    if new_width > 0 {
        let rect = Rect::new(area.right() - new_width, area.y, new_width, 1);
        let label = if area.width >= 60 {
            "Ctrl+T  New connection"
        } else {
            "Ctrl+T  New"
        };
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Right)
                .style(Style::default().fg(TEXT)),
            rect,
        );
        regions.add(rect.x, rect.y, rect.width, rect.height, MouseTarget::Add);
    }
}

pub(super) fn draw_search(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rect = Rect::new(area.x, area.y, area.width, 1);
    frame.render_widget(Block::default().style(Style::default().bg(SURFACE_2)), rect);
    let count = if app.browse_query().is_empty() {
        format!("{} saved", app.profiles().len())
    } else {
        format!(
            "{} / {}",
            app.visible_profile_indices().len(),
            app.profiles().len()
        )
    };
    let count_width = (count.len() as u16 + 2).min(area.width / 3);
    let content = Rect::new(
        area.x + 3,
        area.y,
        area.width.saturating_sub(count_width + 4),
        1,
    );
    frame.render_widget(
        Paragraph::new(" / ").style(Style::default().fg(ACCENT)),
        Rect::new(area.x, area.y, 3, 1),
    );
    if app.browse_query().is_empty() {
        let placeholder = if content.width >= 30 {
            "Find by name, user or host…"
        } else {
            "Type to find…"
        };
        frame.render_widget(
            Paragraph::new(placeholder).style(Style::default().fg(MUTED)),
            content,
        );
    } else {
        let (shown, cursor) = input_window(
            app.browse_query(),
            app.browse_query().chars().count(),
            content.width as usize,
        );
        frame.render_widget(
            Paragraph::new(shown.as_str()).style(Style::default().fg(TEXT)),
            content,
        );
        if app.screen() == Screen::Browse && content.width > 0 {
            let prefix: String = shown.chars().take(cursor).collect();
            let offset = UnicodeWidthStr::width(prefix.as_str())
                .min(content.width.saturating_sub(1) as usize) as u16;
            frame.set_cursor_position((content.x + offset, content.y));
        }
    }
    frame.render_widget(
        Paragraph::new(count)
            .alignment(Alignment::Right)
            .style(Style::default().fg(MUTED)),
        Rect::new(area.right() - count_width, area.y, count_width - 1, 1),
    );
}

pub(super) fn draw_actions(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    regions: &mut MouseRegions,
) {
    if app.selected_profile().is_none() {
        let rect = Rect::new(
            area.x + (area.width.saturating_sub(25)) / 2,
            area.y,
            25.min(area.width),
            1,
        );
        draw_button(frame, rect, "Ctrl+T  New connection", ButtonTone::Accent);
        regions.add(rect.x, rect.y, rect.width, 1, MouseTarget::Add);
        return;
    }
    let connect = Rect::new(area.x, area.y, 16.min(area.width), 1);
    draw_button(frame, connect, "Enter  Connect", ButtonTone::Accent);
    regions.add(connect.x, connect.y, connect.width, 1, MouseTarget::Connect);
    // Shortcuts retain their full spelling. On a small terminal, secondary
    // actions move to their own row instead of stealing space from Connect.
    let secondary_y = area.y + u16::from(area.height > 1);
    let secondary_x = if area.height > 1 {
        area.x
    } else {
        connect.right() + 2
    };
    let edit = Rect::new(secondary_x, secondary_y, 13, 1);
    let delete = Rect::new(edit.right() + 2, secondary_y, 15, 1);
    draw_button(frame, edit, "Ctrl+E  Edit", ButtonTone::Quiet);
    draw_button(frame, delete, "Ctrl+D  Delete", ButtonTone::Quiet);
    regions.add(edit.x, edit.y, edit.width, 1, MouseTarget::Edit);
    regions.add(delete.x, delete.y, delete.width, 1, MouseTarget::Delete);
}

pub(super) fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let row = Rect::new(area.x, area.bottom() - 1, area.width, 1);
    let filtered = !app.browse_query().is_empty();
    let hint = if app.profiles().is_empty() {
        "Add your first server"
    } else if area.width >= 60 {
        if filtered {
            "↑↓ select   ·   Esc clear filter"
        } else {
            "↑↓ select   ·   type to filter"
        }
    } else if area.width >= 48 {
        if filtered {
            "↑↓ select · Esc clear"
        } else {
            "↑↓ select · type to find"
        }
    } else if filtered {
        "Esc clear filter"
    } else {
        "↑↓ select"
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(MUTED)),
        Rect::new(row.x, row.y, row.width.saturating_sub(13), 1),
    );
    frame.render_widget(
        Paragraph::new("Ctrl+C Quit")
            .alignment(Alignment::Right)
            .style(Style::default().fg(MUTED)),
        Rect::new(row.right() - 11, row.y, 11, 1),
    );
}
