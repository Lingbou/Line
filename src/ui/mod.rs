//! Ratatui rendering for the Line application state.

mod browse;
mod form;
mod helpers;
mod modals;
mod theme;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Paragraph, Wrap},
};

use crate::app::{App, MouseRegions, Screen};

use self::{
    browse::draw_browse,
    form::draw_form_modal,
    modals::{draw_delete_key_modal, draw_delete_modal, draw_error_modal},
    theme::{ACCENT, BG, MUTED, TEXT},
};

/// Render one application frame and update its mouse hit boxes.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let mut regions = MouseRegions::default();
    let minimum_height = match app.screen() {
        Screen::Form => 22,
        Screen::ConfirmDeleteKey => 14,
        _ => 14,
    };
    if area.width < 40 || area.height < minimum_height {
        draw_too_small(frame, area, minimum_height);
        app.set_mouse_regions(regions);
        return;
    }
    draw_browse(frame, area, app, &mut regions);

    match app.screen() {
        Screen::Browse => {}
        Screen::Form => draw_form_modal(frame, area, app, &mut regions),
        Screen::ConfirmDelete => draw_delete_modal(frame, area, app, &mut regions),
        Screen::ConfirmDeleteKey => draw_delete_key_modal(frame, area, app, &mut regions),
        Screen::Error => draw_error_modal(frame, area, app, &mut regions),
    }

    app.set_mouse_regions(regions);
}

fn draw_too_small(frame: &mut Frame<'_>, area: Rect, minimum_height: u16) {
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::styled(
                "LINE",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Line::raw(""),
            Line::styled("Terminal too small", Style::default().fg(TEXT)),
            Line::styled(
                format!("Resize to at least 40 × {minimum_height}"),
                Style::default().fg(MUTED),
            ),
        ]))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .style(Style::default().bg(BG)),
        area,
    );
}

#[cfg(test)]
mod tests;
