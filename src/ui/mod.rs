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
    theme::{ACCENT, CANVAS, MUTED, TEXT},
};

/// Render one application frame and update its mouse hit boxes.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(CANVAS)), area);

    let mut regions = MouseRegions::default();
    let form_is_base = matches!(app.screen(), Screen::Form | Screen::ConfirmDeleteKey)
        || matches!(app.screen(), Screen::Error)
            && matches!(
                app.error_return_screen(),
                Screen::Form | Screen::ConfirmDeleteKey
            );
    let minimum_height = if matches!(app.screen(), Screen::Form) {
        22
    } else {
        12
    };
    let terminal_too_small = area.width < 40 || area.height < minimum_height;
    app.set_terminal_too_small(terminal_too_small);
    if terminal_too_small {
        draw_too_small(frame, area, minimum_height);
        app.set_mouse_regions(regions);
        return;
    }

    if form_is_base && area.height >= 22 {
        draw_form_modal(frame, area, app, &mut regions);
    } else if form_is_base {
        // A foreground error/confirmation can fit before its form backdrop
        // does. Keep that dialog usable and show the resize hint behind it.
        draw_too_small(frame, area, 22);
    } else {
        draw_browse(frame, area, app, &mut regions);
    }

    match app.screen() {
        Screen::Browse | Screen::Form => {}
        Screen::ConfirmDelete => {
            regions = MouseRegions::default();
            dim_background(frame, area);
            draw_delete_modal(frame, area, app, &mut regions);
        }
        Screen::ConfirmDeleteKey => {
            regions = MouseRegions::default();
            dim_background(frame, area);
            draw_delete_key_modal(frame, area, app, &mut regions);
        }
        Screen::Error => {
            regions = MouseRegions::default();
            dim_background(frame, area);
            draw_error_modal(frame, area, app, &mut regions);
        }
    };

    app.set_mouse_regions(regions);
}

fn dim_background(frame: &mut Frame<'_>, area: Rect) {
    frame
        .buffer_mut()
        .set_style(area, Style::default().add_modifier(Modifier::DIM));
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
        .style(Style::default().bg(CANVAS)),
        area,
    );
}

#[cfg(test)]
mod tests;
