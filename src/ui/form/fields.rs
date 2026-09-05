use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::app::{FormField, MouseRegions, MouseTarget};

use super::super::{
    helpers::input_window,
    theme::{ACCENT, MUTED, SURFACE_2, TEXT},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn render_input(
    frame: &mut Frame<'_>,
    rect: Rect,
    title: &str,
    value: &str,
    active: bool,
    masked: bool,
    cursor: usize,
    regions: &mut MouseRegions,
    field: FormField,
) {
    if rect.width == 0 || rect.height < 2 {
        return;
    }
    render_field_label(
        frame,
        Rect::new(rect.x, rect.y, rect.width, 1),
        title,
        active,
    );
    let body = Rect::new(
        rect.x,
        rect.y.saturating_add(1),
        rect.width,
        rect.height.saturating_sub(1),
    );
    let content = render_field_surface(frame, body, active);
    let raw_displayed = if masked {
        "•".repeat(value.chars().count())
    } else {
        value.replace(['\n', '\r'], " ")
    };
    let available_width = content.width as usize;
    let (displayed, visible_cursor) = input_window(&raw_displayed, cursor, available_width);
    let placeholder = match field {
        FormField::Name => "Connection 1",
        FormField::Host => "192.168.1.10",
        FormField::Port => "22",
        FormField::Username => "root",
        FormField::Password => "Password",
        FormField::ShowPassword => "",
        _ => "",
    };
    let line = if displayed.is_empty() {
        Line::styled(placeholder, Style::default().fg(MUTED))
    } else {
        Line::styled(displayed.as_str(), Style::default().fg(TEXT))
    };
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(SURFACE_2)),
        content,
    );
    regions.add(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        MouseTarget::Field(field),
    );
    if active && content.width > 0 && content.height > 0 {
        let prefix: String = displayed.chars().take(visible_cursor).collect();
        let offset = UnicodeWidthStr::width(prefix.as_str()) as u16;
        let x = content
            .x
            .saturating_add(offset.min(content.width.saturating_sub(1)));
        frame.set_cursor_position((x, content.y));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_text_box(
    frame: &mut Frame<'_>,
    rect: Rect,
    title: &str,
    value: &str,
    focused: bool,
    editable_cursor: Option<usize>,
    regions: &mut MouseRegions,
    field: FormField,
) {
    if rect.width == 0 || rect.height < 2 {
        return;
    }
    render_field_label(
        frame,
        Rect::new(rect.x, rect.y, rect.width, 1),
        title,
        focused,
    );
    let body = Rect::new(
        rect.x,
        rect.y.saturating_add(1),
        rect.width,
        rect.height.saturating_sub(1),
    );
    let content = render_field_surface(frame, body, focused);
    let displayed = if value.is_empty() {
        "Paste or type here"
    } else {
        value
    };
    let (cursor_row, cursor_col) = editable_cursor
        .map(|cursor| wrapped_cursor(value, cursor, content.width as usize))
        .unwrap_or((0, 0));
    let scroll = cursor_row.saturating_sub(content.height.saturating_sub(1) as usize) as u16;
    frame.render_widget(
        Paragraph::new(hard_wrapped_lines(displayed, content.width as usize))
            .scroll((scroll, 0))
            .style(
                Style::default()
                    .fg(if value.is_empty() { MUTED } else { TEXT })
                    .bg(SURFACE_2),
            ),
        content,
    );
    if editable_cursor.is_some() && content.width > 0 && content.height > 0 {
        // Placeholder text is guidance, so an empty value still places the
        // cursor at the first editable cell. Multiline values scroll just
        // enough to keep the real insertion point in the input body.
        let x = content
            .x
            .saturating_add((cursor_col as u16).min(content.width.saturating_sub(1)));
        let y = content.y.saturating_add(
            (cursor_row.saturating_sub(scroll as usize) as u16)
                .min(content.height.saturating_sub(1)),
        );
        frame.set_cursor_position((x, y));
    }
    regions.add(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        MouseTarget::Field(field),
    );
}

fn render_field_label(frame: &mut Frame<'_>, rect: Rect, title: &str, active: bool) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                title,
                Style::default()
                    .fg(if active { ACCENT } else { MUTED })
                    .add_modifier(if active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ])),
        rect,
    );
}

fn render_field_surface(frame: &mut Frame<'_>, rect: Rect, active: bool) -> Rect {
    if rect.width == 0 || rect.height == 0 {
        return Rect::new(rect.x, rect.y, 0, 0);
    }
    frame.render_widget(Block::default().style(Style::default().bg(SURFACE_2)), rect);
    frame.render_widget(
        Paragraph::new(if active { "▎" } else { " " })
            .style(Style::default().fg(ACCENT).bg(SURFACE_2)),
        Rect::new(rect.x, rect.y, 1, rect.height),
    );
    let padding = 2.min(rect.width);
    Rect::new(
        rect.x.saturating_add(padding),
        rect.y,
        rect.width.saturating_sub(padding + 1),
        rect.height,
    )
}

fn wrapped_cursor(value: &str, cursor: usize, width: usize) -> (usize, usize) {
    let prefix = prefix_at_grapheme_boundary(value, cursor);
    let lines = hard_wrapped_strings(prefix, width);
    let row = lines.len().saturating_sub(1);
    let column = lines
        .last()
        .map(|line| UnicodeWidthStr::width(line.as_str()))
        .unwrap_or(0);
    (row, column)
}

fn prefix_at_grapheme_boundary(value: &str, cursor: usize) -> &str {
    let cursor = cursor.min(value.chars().count());
    let mut consumed_chars = 0;
    let mut end_byte = 0;
    for (byte, grapheme) in value.grapheme_indices(true) {
        let grapheme_end = consumed_chars + grapheme.chars().count();
        if cursor < grapheme_end {
            break;
        }
        consumed_chars = grapheme_end;
        end_byte = byte + grapheme.len();
        if cursor == consumed_chars {
            break;
        }
    }
    &value[..end_byte]
}

fn hard_wrapped_lines(value: &str, width: usize) -> Vec<Line<'static>> {
    hard_wrapped_strings(value, width)
        .into_iter()
        .map(Line::raw)
        .collect()
}

fn hard_wrapped_strings(value: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut line = String::new();
    let mut column = 0;
    let mut wrapped_at_boundary = false;
    for grapheme in value.graphemes(true) {
        if matches!(grapheme, "\n" | "\r\n") {
            if !wrapped_at_boundary {
                lines.push(std::mem::take(&mut line));
            }
            column = 0;
            wrapped_at_boundary = false;
            continue;
        }
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if column + grapheme_width > width {
            lines.push(std::mem::take(&mut line));
            column = 0;
        }
        wrapped_at_boundary = false;
        line.push_str(grapheme);
        column += grapheme_width;
        if column == width {
            lines.push(std::mem::take(&mut line));
            column = 0;
            wrapped_at_boundary = true;
        }
    }
    lines.push(line);
    lines
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn text_box_hard_wraps_spaces_to_match_the_visible_cursor() {
        let value = "alpha beta";
        let mut terminal = render_active_text_box(value, FormField::KeyValue, 4);

        assert_eq!(content_row(&terminal, 1), "alpha be");
        assert_eq!(content_row(&terminal, 2), "ta");
        assert_eq!(terminal.get_cursor_position().unwrap(), (4, 2).into());
    }

    #[test]
    fn text_box_places_a_path_cursor_after_an_exact_width_line() {
        let value = "/tmp/private-key";
        let mut terminal = render_active_text_box(value, FormField::KeyValue, 4);

        assert_eq!(content_row(&terminal, 1), "/tmp/pri");
        assert_eq!(content_row(&terminal, 2), "vate-key");
        assert_eq!(content_row(&terminal, 3), "");
        assert_eq!(terminal.get_cursor_position().unwrap(), (2, 3).into());
    }

    #[test]
    fn text_box_hard_wraps_words_after_an_explicit_newline() {
        let value = "PRIVATE\nalpha beta";
        let mut terminal = render_active_text_box(value, FormField::PublicKey, 3);

        assert_eq!(content_row(&terminal, 1), "alpha be");
        assert_eq!(content_row(&terminal, 2), "ta");
        assert_eq!(terminal.get_cursor_position().unwrap(), (4, 2).into());
    }

    #[test]
    fn explicit_newline_after_an_exact_width_line_does_not_add_a_blank_row() {
        let value = "12345678\nX";
        let mut terminal = render_active_text_box(value, FormField::KeyValue, 4);

        assert_eq!(content_row(&terminal, 1), "12345678");
        assert_eq!(content_row(&terminal, 2), "X");
        assert_eq!(content_row(&terminal, 3), "");
        assert_eq!(terminal.get_cursor_position().unwrap(), (3, 2).into());
    }

    #[test]
    fn text_box_content_keeps_its_foreground_and_background_colors() {
        let terminal = render_active_text_box("K", FormField::KeyValue, 4);
        let cell = &terminal.backend().buffer()[(2, 1)];

        assert_eq!(cell.fg, TEXT);
        assert_eq!(cell.bg, SURFACE_2);
    }

    #[test]
    fn text_box_keeps_a_combining_grapheme_on_one_visual_line() {
        let value = "e\u{301}";
        let mut terminal = render_active_text_box_at_width(value, FormField::KeyValue, 3, 4);
        let buffer = terminal.backend().buffer();

        assert_eq!(buffer[(2, 1)].symbol(), value);
        assert_eq!(buffer[(2, 2)].symbol(), " ");
        assert_eq!(terminal.get_cursor_position().unwrap(), (2, 2).into());
    }

    #[test]
    fn focused_read_only_text_box_keeps_the_human_label_at_the_top() {
        let backend = TestBackend::new(11, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut regions = MouseRegions::default();
        terminal
            .draw(|frame| {
                render_text_box(
                    frame,
                    Rect::new(0, 0, 11, 3),
                    "Existing key",
                    "work-key · used by 1 connection",
                    true,
                    None,
                    &mut regions,
                    FormField::KeyValue,
                );
            })
            .unwrap();

        assert_eq!(content_row(&terminal, 1), "work-key");
        assert_eq!(terminal.backend().buffer()[(0, 1)].symbol(), "▎");
    }

    fn render_active_text_box(value: &str, field: FormField, height: u16) -> Terminal<TestBackend> {
        render_active_text_box_at_width(value, field, height, 11)
    }

    fn render_active_text_box_at_width(
        value: &str,
        field: FormField,
        height: u16,
        width: u16,
    ) -> Terminal<TestBackend> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut regions = MouseRegions::default();
        terminal
            .draw(|frame| {
                render_text_box(
                    frame,
                    Rect::new(0, 0, width, height),
                    "Key material",
                    value,
                    true,
                    Some(value.chars().count()),
                    &mut regions,
                    field,
                );
            })
            .unwrap();
        terminal
    }

    fn content_row(terminal: &Terminal<TestBackend>, y: u16) -> String {
        let buffer = terminal.backend().buffer();
        (2..10)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }
}
