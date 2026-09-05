use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

use crate::app::{AuthDraft, FormState, KeySource, MouseRegions, MouseTarget, SaveMode};

use super::super::theme::{ACCENT, ACCENT_2, BORDER, DANGER, MUTED, SURFACE, TEXT};

const EDITOR_MAX_WIDTH: u16 = 96;
pub(super) const ROOMY_BODY_WIDTH: u16 = 52;

pub(super) struct FormLayout {
    pub(super) editor: Rect,
    pub(super) header: Rect,
    pub(super) body: Rect,
    pub(super) footer: Rect,
}

pub(super) fn form_layout(area: Rect, form: &FormState) -> FormLayout {
    let width = area
        .width
        .saturating_sub(if area.width >= 48 { 4 } else { 0 })
        .min(EDITOR_MAX_WIDTH);
    let roomy = width.saturating_sub(4) >= ROOMY_BODY_WIDTH;
    let body_height = match (&form.auth, roomy) {
        (AuthDraft::Password { .. }, _) => 12,
        (
            AuthDraft::Key {
                source: KeySource::Existing,
                ..
            },
            true,
        ) => 16,
        (
            AuthDraft::Key {
                source: KeySource::Existing,
                ..
            },
            false,
        ) => 14,
        (
            AuthDraft::Key {
                source: KeySource::Paste,
                ..
            },
            true,
        ) => 19,
        (AuthDraft::Key { .. }, true) => 17,
        (AuthDraft::Key { .. }, false) => 15,
    };
    let header_height: u16 = if roomy { 3 } else { 2 };
    let preferred_height = body_height + header_height + 5;
    let normal_height = area.height.min(preferred_height);
    let error_rows = form
        .validation_error
        .as_deref()
        .map(|error| {
            u16::try_from(
                Paragraph::new(error)
                    .wrap(Wrap { trim: false })
                    .line_count(width.saturating_sub(6)),
            )
            .unwrap_or(u16::MAX)
        })
        .unwrap_or(0);
    let extra_footer_rows = error_rows.saturating_add(1).saturating_sub(3);
    let grow_by = extra_footer_rows.min(area.height.saturating_sub(normal_height));
    let borrow_header_rows = extra_footer_rows
        .saturating_sub(grow_by)
        .min(header_height.saturating_sub(1));
    let height = normal_height.saturating_add(grow_by);
    let header_height = header_height.saturating_sub(borrow_header_rows);
    let footer_height = 3 + grow_by + borrow_header_rows;
    let editor = Rect::new(
        area.x.saturating_add(area.width.saturating_sub(width) / 2),
        area.y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    );
    let inner = inset(editor, 1, 1);
    let header_height = header_height.min(inner.height);
    let footer_height = footer_height.min(inner.height.saturating_sub(header_height));
    let padding = 1;
    let header = horizontal_inset(
        Rect::new(inner.x, inner.y, inner.width, header_height),
        padding + 2,
    );
    let body = horizontal_inset(
        Rect::new(
            inner.x,
            inner.y.saturating_add(header_height),
            inner.width,
            inner.height.saturating_sub(header_height + footer_height),
        ),
        padding,
    );
    let footer = horizontal_inset(
        Rect::new(
            inner.x,
            inner.bottom().saturating_sub(footer_height),
            inner.width,
            footer_height,
        ),
        padding,
    );
    FormLayout {
        editor,
        header,
        body,
        footer,
    }
}

pub(super) fn draw_editor_frame(frame: &mut Frame<'_>, editor: Rect) {
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(SURFACE)),
        editor,
    );
}

pub(super) fn draw_sheet_header(frame: &mut Frame<'_>, rect: Rect, form: &FormState) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let title = match form.mode {
        SaveMode::Add => "New connection",
        SaveMode::Edit => "Edit connection",
    };
    frame.render_widget(
        Paragraph::new(title).style(Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Rect::new(rect.x, rect.y, rect.width, 1),
    );
    if rect.height >= 3 {
        frame.render_widget(
            Paragraph::new("Choose how to connect to this server.")
                .style(Style::default().fg(MUTED)),
            Rect::new(rect.x, rect.y + 1, rect.width, 1),
        );
    }
}

pub(super) fn draw_sheet_footer(
    frame: &mut Frame<'_>,
    rect: Rect,
    form: &FormState,
    regions: &mut MouseRegions,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let mut actions_y = rect.y.saturating_add(1).min(rect.bottom() - 1);
    if let Some(error) = &form.validation_error {
        let message_area = Rect::new(
            rect.x + 2,
            rect.y,
            rect.width.saturating_sub(2),
            rect.height,
        );
        let message = Paragraph::new(error.as_str())
            .style(Style::default().fg(DANGER))
            .wrap(Wrap { trim: false });
        let height = u16::try_from(message.line_count(message_area.width))
            .unwrap_or(u16::MAX)
            .max(1)
            .min(rect.height);
        frame.render_widget(
            message,
            Rect::new(message_area.x, message_area.y, message_area.width, height),
        );
        actions_y = rect.y.saturating_add(height);
    }
    if actions_y >= rect.bottom() {
        return;
    }
    let save_width = 14.min(rect.width);
    let cancel_width = 14.min(rect.width.saturating_sub(save_width));
    let gap = u16::from(rect.width > save_width + cancel_width);
    let group_width = save_width + cancel_width + gap;
    let right_padding = u16::from(rect.width > group_width);
    let x = rect.right().saturating_sub(group_width + right_padding);
    let save = Rect::new(x, actions_y, save_width, 1);
    let cancel = Rect::new(save.right().saturating_add(gap), actions_y, cancel_width, 1);
    draw_action(frame, save, "Enter  Save", true);
    draw_action(frame, cancel, "Esc  Cancel", false);
    regions.add(save.x, save.y, save.width, 1, MouseTarget::Save);
    regions.add(cancel.x, cancel.y, cancel.width, 1, MouseTarget::Cancel);
    if form.validation_error.is_none() && rect.width >= 54 {
        frame.render_widget(
            Paragraph::new("Tab  next field").style(Style::default().fg(MUTED)),
            Rect::new(rect.x + 2, actions_y, x.saturating_sub(rect.x + 3), 1),
        );
    }
}

fn draw_action(frame: &mut Frame<'_>, rect: Rect, label: &str, primary: bool) {
    frame.render_widget(
        Paragraph::new(label).alignment(Alignment::Center).style(
            Style::default()
                .fg(if primary { ACCENT } else { MUTED })
                .bg(if primary { ACCENT_2 } else { SURFACE })
                .add_modifier(if primary {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        rect,
    );
}

fn inset(rect: Rect, horizontal: u16, vertical: u16) -> Rect {
    Rect::new(
        rect.x.saturating_add(horizontal.min(rect.width)),
        rect.y.saturating_add(vertical.min(rect.height)),
        rect.width.saturating_sub(horizontal.saturating_mul(2)),
        rect.height.saturating_sub(vertical.saturating_mul(2)),
    )
}

fn horizontal_inset(rect: Rect, margin: u16) -> Rect {
    Rect::new(
        rect.x.saturating_add(margin.min(rect.width)),
        rect.y,
        rect.width.saturating_sub(margin.saturating_mul(2)),
        rect.height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn editor_stays_content_sized_on_large_terminals() {
        let app = App::new(Vec::new());
        let layout = form_layout(Rect::new(0, 0, 240, 64), app.form().unwrap());
        assert_eq!(layout.editor, Rect::new(72, 22, 96, 20));
        assert_parts_are_inside_editor(layout);
    }

    #[test]
    fn minimum_key_editor_keeps_room_for_both_key_inputs_and_helper() {
        let mut app = App::new(Vec::new());
        app.form_mut().unwrap().auth = AuthDraft::Key {
            source: KeySource::Paste,
            value: String::new(),
            public_key: None,
        };
        let layout = form_layout(Rect::new(0, 0, 40, 22), app.form().unwrap());
        assert_eq!(layout.editor, Rect::new(0, 0, 40, 22));
        assert_eq!(layout.body.height, 15);
        assert_parts_are_inside_editor(layout);
    }

    #[test]
    fn breakpoint_widths_preserve_key_inputs_and_save() {
        for width in 56..=64 {
            for height in [22, 24] {
                for source in [KeySource::Import, KeySource::Paste] {
                    let mut app = App::new(Vec::new());
                    app.form_mut().unwrap().auth = AuthDraft::Key {
                        source,
                        value: "/tmp/private".into(),
                        public_key: Some("/tmp/public".into()),
                    };
                    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                    terminal
                        .draw(|frame| crate::ui::draw(frame, &mut app))
                        .unwrap();
                    let buffer = terminal.backend().buffer();
                    let text = (0..height)
                        .map(|y| {
                            (0..width)
                                .map(|x| buffer[(x, y)].symbol())
                                .collect::<String>()
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    for visible in ["/tmp/private", "/tmp/public", "Enter  Save", "Esc  Cancel"] {
                        assert!(
                            text.contains(visible),
                            "{width}x{height} {source:?} is missing {visible}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn compact_footer_shows_a_complete_validation_error_without_shrinking_inputs() {
        let mut app = App::new(Vec::new());
        app.form_mut().unwrap().auth = AuthDraft::Key {
            source: KeySource::Paste,
            value: String::new(),
            public_key: None,
        };
        let normal = form_layout(Rect::new(0, 0, 40, 22), app.form().unwrap());
        app.form_mut().unwrap().validation_error =
            Some("Connection changed elsewhere; cancel and edit it again before saving.".into());
        let layout = form_layout(Rect::new(0, 0, 40, 22), app.form().unwrap());
        assert_eq!(normal.body.width, layout.body.width);
        assert_eq!(normal.body.height, layout.body.height);
        let mut terminal = Terminal::new(TestBackend::new(40, 22)).unwrap();
        let mut regions = MouseRegions::default();
        terminal
            .draw(|frame| {
                draw_sheet_footer(frame, layout.footer, app.form().unwrap(), &mut regions)
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let message = (layout.footer.y..layout.footer.bottom())
            .map(|y| {
                (layout.footer.x..layout.footer.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            message
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains("Connection changed elsewhere; cancel and edit it again before saving.")
        );
    }

    #[test]
    fn three_line_errors_keep_complete_inputs_and_actions_at_forty_columns() {
        let error = "A connection named Production already exists. Choose a different name.";
        for is_key in [false, true] {
            let mut app = App::new(Vec::new());
            if is_key {
                app.form_mut().unwrap().auth = AuthDraft::Key {
                    source: KeySource::Import,
                    value: "/tmp/private".into(),
                    public_key: Some("/tmp/public".into()),
                };
            }
            let normal = form_layout(Rect::new(0, 0, 40, 22), app.form().unwrap());
            app.form_mut().unwrap().validation_error = Some(error.into());
            let layout = form_layout(Rect::new(0, 0, 40, 22), app.form().unwrap());
            assert_eq!(normal.body.height, layout.body.height);
            let mut terminal = Terminal::new(TestBackend::new(40, 22)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let text = (0..22)
                .map(|y| (1..39).map(|x| buffer[(x, y)].symbol()).collect::<String>())
                .collect::<Vec<_>>()
                .join(" ");
            let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(normalized.contains(error));
            assert!(normalized.contains("Enter Save"));
            assert!(normalized.contains("Esc Cancel"));
            if is_key {
                assert!(normalized.contains("/tmp/private"));
                assert!(normalized.contains("/tmp/public"));
            }
        }
    }

    fn assert_parts_are_inside_editor(layout: FormLayout) {
        for rect in [layout.header, layout.body, layout.footer] {
            assert!(rect.x >= layout.editor.x && rect.y >= layout.editor.y);
            assert!(
                rect.right() <= layout.editor.right() && rect.bottom() <= layout.editor.bottom()
            );
        }
        assert!(layout.header.bottom() <= layout.body.y);
        assert!(layout.body.bottom() <= layout.footer.y);
    }
}
