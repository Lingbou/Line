use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::App,
    config::AuthMethod,
    ui::{
        helpers::truncate,
        theme::{BORDER, MUTED, SUCCESS, TEXT},
    },
};

use super::list::endpoint;

/// A quiet context line, not a second copy of the selected connection card.
pub(super) fn draw_detail(frame: &mut Frame<'_>, area: Rect, app: &App, show_endpoint: bool) {
    if !show_endpoint {
        frame.render_widget(
            Paragraph::new("─".repeat(area.width as usize)).style(Style::default().fg(BORDER)),
            Rect::new(area.x, area.y, area.width, 1),
        );
        // Two-line connection rows already include the endpoint. Keep their
        // compact action divider quiet instead of duplicating that address.
        if area.height == 1 {
            return;
        }
    }
    let y = area.y + u16::from(area.height >= 2);
    if area.height > 1
        && let Some(message) = app.status_message()
    {
        frame.render_widget(
            Paragraph::new(truncate(message, area.width as usize))
                .style(Style::default().fg(SUCCESS)),
            Rect::new(area.x, y, area.width, 1),
        );
        return;
    }
    let Some(profile) = app.selected_profile() else {
        return;
    };
    let text = if show_endpoint {
        Line::styled(
            truncate(&endpoint(profile), area.width as usize),
            Style::default().fg(TEXT),
        )
    } else {
        let (label, value) = match &profile.auth {
            AuthMethod::Password { .. } => ("Password", "Saved for this connection".to_owned()),
            AuthMethod::Key { private_key, .. } => ("SSH key", private_key.display().to_string()),
        };
        Line::from(vec![
            Span::styled(format!("{label}  "), Style::default().fg(MUTED)),
            Span::styled(
                truncate(
                    &value,
                    area.width.saturating_sub(label.len() as u16 + 2) as usize,
                ),
                Style::default().fg(TEXT),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(text), Rect::new(area.x, y, area.width, 1));
}
