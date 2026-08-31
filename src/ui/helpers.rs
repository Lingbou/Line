use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::theme::{ACCENT, BG, DANGER, SURFACE_2, TEXT};

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
        ButtonTone::Accent => (BG, ACCENT, ACCENT),
        ButtonTone::Quiet => (TEXT, SURFACE_2, SURFACE_2),
        ButtonTone::Danger => (Color::White, DANGER, DANGER),
    };
    let paragraph = Paragraph::new(truncate(
        label,
        rect.width
            .saturating_sub(if rect.height >= 2 { 2 } else { 0 }) as usize,
    ))
    .alignment(Alignment::Center)
    .style(
        Style::default()
            .fg(foreground)
            .bg(background)
            .add_modifier(Modifier::BOLD),
    );
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
    let chars: Vec<char> = value.chars().collect();
    let cursor = cursor.min(chars.len());
    if chars.len() <= width {
        return (value.to_owned(), cursor);
    }
    let mut start = cursor.saturating_sub(width.saturating_sub(1));
    if start + width < cursor + 1 {
        start = cursor + 1 - width;
    }
    let end = (start + width).min(chars.len());
    if end - start < width {
        start = end.saturating_sub(width);
    }
    let mut shown: String = chars[start..end].iter().collect();
    let mut visible_cursor = cursor.saturating_sub(start);
    if start > 0 {
        if shown.chars().count() >= width {
            shown.remove(0);
        }
        shown.insert(0, '…');
        visible_cursor = visible_cursor.saturating_add(1);
    }
    (shown, visible_cursor.min(width))
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
    for character in value.chars() {
        let width = character.width().unwrap_or(0);
        if used + width > content_width {
            break;
        }
        result.push(character);
        used += width;
    }
    result.push('…');
    result
}
