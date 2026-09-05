mod auth;
mod fields;
mod layout;
mod sections;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Block, Clear},
};

use crate::app::{App, FormField, MouseRegions, Screen};

use super::theme::CANVAS;
#[cfg(test)]
pub(super) use auth::existing_key_label;
use layout::{draw_editor_frame, draw_sheet_footer, draw_sheet_header, form_layout};
use sections::draw_form_fields;

pub(super) fn draw_form_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    regions: &mut MouseRegions,
) {
    let Some(active_form) = app.form() else {
        return;
    };
    // Confirmation and error dialogs are drawn over the form. Render their
    // background copy without a focused text field so it cannot leak a
    // visible terminal cursor through the modal.
    let inactive_form = (!matches!(app.screen(), Screen::Form)).then(|| {
        let mut form = active_form.clone();
        form.field = FormField::KeySource;
        form
    });
    let form = inactive_form.as_ref().unwrap_or(active_form);

    // The form is a screen, not an overlay. Besides hiding the browse screen,
    // resetting the hit boxes prevents clicks on sheet whitespace from
    // activating controls that are visually behind it.
    *regions = MouseRegions::default();
    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(Style::default().bg(CANVAS)), area);

    let layout = form_layout(area, form);
    draw_editor_frame(frame, layout.editor);
    draw_sheet_header(frame, layout.header, form);

    draw_form_fields(frame, layout.body, app, form, regions);
    draw_sheet_footer(frame, layout.footer, form, regions);
}
