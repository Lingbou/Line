use ratatui::{Frame, layout::Rect, style::Style, widgets::Paragraph};

use crate::app::{App, AuthDraft, FormField, FormState, KeySource, MouseRegions};

use super::{
    super::theme::MUTED,
    auth::{
        key_value, password_label, render_existing_key_action, render_key_sources,
        render_public_key, render_segmented_auth, render_show_password,
    },
    fields::{render_input, render_text_box},
    layout::ROOMY_BODY_WIDTH,
};

pub(super) fn draw_form_fields(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    form: &FormState,
    regions: &mut MouseRegions,
) {
    let roomy = area.width >= ROOMY_BODY_WIDTH;
    let input =
        |frame: &mut Frame<'_>, rect, label, value: &str, field, regions: &mut MouseRegions| {
            render_input(
                frame,
                rect,
                label,
                value,
                form.field == field,
                false,
                form.cursor,
                regions,
                field,
            );
        };

    if roomy {
        let (name, username) = split_trailing_field(row(area, 0, 2), 24);
        input(frame, name, "Name", &form.name, FormField::Name, regions);
        input(
            frame,
            username,
            "Username",
            &form.username,
            FormField::Username,
            regions,
        );
    } else {
        input(
            frame,
            row(area, 0, 2),
            "Name",
            &form.name,
            FormField::Name,
            regions,
        );
        input(
            frame,
            row(area, 2, 2),
            "Username",
            &form.username,
            FormField::Username,
            regions,
        );
    }
    let (host, port) = split_trailing_field(row(area, if roomy { 3 } else { 4 }, 2), 9);
    input(frame, host, "Host", &form.host, FormField::Host, regions);
    input(frame, port, "Port", &form.port, FormField::Port, regions);
    let auth_y = if roomy { 7 } else { 8 };
    hint(frame, row(area, auth_y - 1, 1), "Authentication");
    render_segmented_auth(frame, row(area, auth_y, 1), form, regions);
    match &form.auth {
        AuthDraft::Password { password } => {
            let password_y = auth_y + if roomy { 2 } else { 1 };
            render_input(
                frame,
                row(area, password_y, 2),
                password_label(form.mode),
                password,
                form.field == FormField::Password,
                !form.show_password,
                form.cursor,
                regions,
                FormField::Password,
            );
            render_show_password(frame, row(area, password_y + 2, 1), form, regions);
        }
        AuthDraft::Key {
            source,
            value,
            public_key,
        } => {
            let extra_rows = area.height.saturating_sub(auth_y + 7);
            let group_gaps = if roomy { extra_rows.min(2) } else { 0 };
            let multiline_extra = if *source == KeySource::Paste {
                extra_rows.saturating_sub(group_gaps).min(2)
            } else {
                0
            };
            let gaps = if roomy {
                extra_rows.saturating_sub(multiline_extra).min(3)
            } else {
                0
            };
            let source_y = auth_y + 1 + u16::from(gaps > 1);
            render_key_sources(
                frame,
                row(area, source_y, 1),
                *source,
                form.field == FormField::KeySource,
                regions,
            );
            let private_y = source_y + 1 + u16::from(gaps > 2);
            let private_height = 2 + multiline_extra;
            let (label, displayed) = key_value(app, *source, value);
            render_text_box(
                frame,
                row(area, private_y, private_height),
                label,
                &displayed,
                form.field == FormField::KeyValue,
                (form.field == FormField::KeyValue && *source != KeySource::Existing)
                    .then_some(form.cursor),
                regions,
                FormField::KeyValue,
            );
            let next_y = private_y + private_height + u16::from(gaps > 0);
            if *source == KeySource::Existing {
                render_existing_key_action(frame, row(area, next_y, 1), app, value, regions);
                hint(frame, row(area, next_y + 1, 1), "← / → choose a saved key");
            } else {
                render_public_key(
                    frame,
                    row(area, next_y, 2),
                    *source,
                    public_key.as_deref().unwrap_or(""),
                    form,
                    regions,
                );
                let help = if roomy && *source == KeySource::Import {
                    "Blank public key is derived. Tab completes paths."
                } else {
                    "Public key is derived when blank."
                };
                hint(frame, row(area, next_y + 2, 1), help);
            }
        }
    }
}

fn hint(frame: &mut Frame<'_>, rect: Rect, text: &str) {
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(MUTED)),
        Rect::new(
            rect.x + 2,
            rect.y,
            rect.width.saturating_sub(2),
            rect.height,
        ),
    );
}

fn split_trailing_field(area: Rect, trailing_width: u16) -> (Rect, Rect) {
    let gap = 2;
    let trailing_width = trailing_width.min(area.width);
    let leading_width = area.width.saturating_sub(trailing_width + gap);
    (
        Rect::new(area.x, area.y, leading_width, area.height),
        Rect::new(
            area.right().saturating_sub(trailing_width),
            area.y,
            trailing_width,
            area.height,
        ),
    )
}

fn row(area: Rect, offset: u16, height: u16) -> Rect {
    let y = area.y.saturating_add(offset);
    if y >= area.bottom() {
        return Rect::new(area.x, area.bottom(), area.width, 0);
    }
    Rect::new(area.x, y, area.width, height.min(area.bottom() - y))
}
