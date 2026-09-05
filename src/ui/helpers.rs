use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::theme::{ACCENT, ACCENT_2, BG, BORDER, DANGER, MUTED};

#[derive(Clone, Copy)]
pub(super) enum ButtonTone {
    Accent,
    Quiet,
    Danger,
}

pub(super) fn draw_button(frame: &mut Frame<'_>, rect: Rect, label: &str, tone: ButtonTone) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let (foreground, background, border) = match tone {
        ButtonTone::Accent => (ACCENT, ACCENT_2, ACCENT),
        ButtonTone::Quiet => (MUTED, BG, BORDER),
        ButtonTone::Danger => (DANGER, BG, DANGER),
    };
    let paragraph = Paragraph::new(truncate(
        label,
        rect.width
            .saturating_sub(if rect.height >= 2 { 2 } else { 0 }) as usize,
    ))
    .alignment(Alignment::Center)
    .style(Style::default().fg(foreground).bg(background).add_modifier(
        if matches!(tone, ButtonTone::Quiet) {
            Modifier::empty()
        } else {
            Modifier::BOLD
        },
    ));
    if rect.height >= 2 {
        frame.render_widget(
            paragraph.block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border))
                    .style(Style::default().bg(background)),
            ),
            rect,
        );
    } else {
        frame.render_widget(paragraph, rect);
    }
}

pub(super) fn input_window(value: &str, cursor: usize, width: usize) -> (String, usize) {
    if width == 0 {
        return (String::new(), 0);
    }
    let mut char_offset = 0;
    let graphemes = value
        .graphemes(true)
        .map(|text| {
            let start = char_offset;
            char_offset += text.chars().count();
            DisplayGrapheme {
                text,
                start,
                end: char_offset,
                width: UnicodeWidthStr::width(text),
            }
        })
        .collect::<Vec<_>>();
    let cursor = cursor.min(char_offset);
    let focus = graphemes
        .iter()
        .position(|grapheme| cursor < grapheme.end)
        .unwrap_or(graphemes.len());
    // A terminal cannot place a cursor inside one displayed grapheme. If the
    // character-indexed editor lands inside one, show it at that grapheme's
    // leading edge without splitting the visible text.
    let cursor = graphemes
        .get(focus)
        .map(|grapheme| grapheme.start)
        .unwrap_or(char_offset);
    let prefix_width = graphemes[..focus]
        .iter()
        .map(|grapheme| grapheme.width)
        .sum::<usize>();
    let focus_width = graphemes
        .get(focus)
        .map(|grapheme| grapheme.width)
        .unwrap_or(1)
        .max(1);
    let focus_fits_from_start = prefix_width.saturating_add(focus_width) <= width;
    if UnicodeWidthStr::width(value) <= width && focus_fits_from_start {
        return (value.to_owned(), cursor);
    }

    let (start, show_leading_ellipsis) = if focus_fits_from_start {
        (0, false)
    } else {
        let ellipsis_width = UnicodeWidthChar::width('…').unwrap_or(1);
        let show_leading_ellipsis =
            focus > 0 && ellipsis_width.saturating_add(focus_width) <= width;
        let before_cursor_width =
            width
                .saturating_sub(focus_width)
                .saturating_sub(if show_leading_ellipsis {
                    ellipsis_width
                } else {
                    0
                });
        let mut start = focus;
        let mut used = 0;
        while start > 0 {
            let grapheme_width = graphemes[start - 1].width;
            if used + grapheme_width > before_cursor_width {
                break;
            }
            start -= 1;
            used += grapheme_width;
        }
        (start, show_leading_ellipsis && start > 0)
    };

    let mut shown = String::new();
    let mut used = 0;
    if show_leading_ellipsis {
        shown.push('…');
        used = UnicodeWidthChar::width('…').unwrap_or(1);
    }
    for grapheme in &graphemes[start..] {
        if used + grapheme.width > width {
            break;
        }
        shown.push_str(grapheme.text);
        used += grapheme.width;
    }

    let start_char = graphemes
        .get(start)
        .map(|grapheme| grapheme.start)
        .unwrap_or(char_offset);
    let visible_cursor = cursor.saturating_sub(start_char) + usize::from(show_leading_ellipsis);
    (shown, visible_cursor)
}

#[derive(Clone, Copy)]
struct DisplayGrapheme<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    width: usize,
}

pub(super) fn modal_rect(area: Rect, max_width: u16, desired_height: u16) -> Rect {
    let width = max_width.min(area.width.saturating_sub(2)).max(1);
    let height = desired_height.min(area.height.saturating_sub(2)).max(1);
    centered_fixed(area, width, height)
}

pub(super) fn centered_fixed(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x.saturating_add(area.width.saturating_sub(width) / 2),
        area.y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    )
}

pub(super) fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(horizontal.min(area.width)),
        area.y.saturating_add(vertical.min(area.height)),
        area.width.saturating_sub(horizontal.saturating_mul(2)),
        area.height.saturating_sub(vertical.saturating_mul(2)),
    )
}

pub(super) fn truncate(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".into();
    }
    let content_width = max_width - 1;
    let mut result = String::new();
    let mut used = 0;
    for grapheme in value.graphemes(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if used + width > content_width {
            break;
        }
        result.push_str(grapheme);
        used += width;
    }
    result.push('…');
    result
}

#[cfg(test)]
mod tests {
    use unicode_width::UnicodeWidthStr;

    use super::{input_window, truncate};

    #[test]
    fn input_window_scrolls_wide_characters_by_terminal_columns() {
        let (shown, cursor) = input_window("服务器连接名称", 7, 6);

        assert_eq!(shown, "…名称");
        assert_eq!(UnicodeWidthStr::width(shown.as_str()), 5);
        assert_eq!(cursor, 3);
    }

    #[test]
    fn input_window_keeps_a_character_cursor_for_mixed_width_text() {
        let (shown, cursor) = input_window("ab上海cdef", 5, 7);

        assert_eq!(shown, "…上海cd");
        assert_eq!(UnicodeWidthStr::width(shown.as_str()), 7);
        assert_eq!(cursor, 4);
        assert_eq!(shown.chars().take(cursor).collect::<String>(), "…上海c");
    }

    #[test]
    fn input_window_reserves_a_column_for_an_end_cursor_at_exact_width() {
        let (shown, cursor) = input_window("abcd", 4, 4);

        assert_eq!(shown, "…cd");
        assert_eq!(UnicodeWidthStr::width(shown.as_str()), 3);
        assert_eq!(cursor, 3);
    }

    #[test]
    fn input_window_keeps_the_wide_character_after_the_cursor_whole() {
        let (shown, cursor) = input_window("abcde中f", 5, 6);

        assert_eq!(shown, "…cde中");
        assert_eq!(UnicodeWidthStr::width(shown.as_str()), 6);
        assert_eq!(cursor, 4);
        assert_eq!(shown.chars().nth(cursor), Some('中'));
    }

    #[test]
    fn input_window_keeps_a_fitting_emoji_grapheme_visible() {
        let value = "👩‍💻".repeat(9);
        let (shown, cursor) = input_window(&value, value.chars().count(), 35);

        assert_eq!(shown, value);
        assert_eq!(UnicodeWidthStr::width(shown.as_str()), 18);
        assert_eq!(cursor, shown.chars().count());
    }

    #[test]
    fn truncation_never_splits_a_zwj_emoji_grapheme() {
        assert_eq!(truncate("👨‍👩‍👧‍👦xyz", 3), "👨‍👩‍👧‍👦…");
    }
}
