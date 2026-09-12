use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::{
    app::{App, MouseRegions, MouseTarget},
    config::{AuthMethod, Profile},
    ui::{
        helpers::truncate,
        theme::{ACCENT, ACCENT_2, BG, MUTED, TEXT},
    },
};

pub(super) fn draw_connection_list(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    regions: &mut MouseRegions,
    dense: bool,
) {
    let indices = app.visible_profile_indices();
    if indices.is_empty() {
        let empty = app.profiles().is_empty();
        let y = area.y + area.height.saturating_sub(3) / 2;
        frame.render_widget(
            Paragraph::new(if empty {
                "No connections yet"
            } else {
                "No matching connections"
            })
            .alignment(Alignment::Center)
            .style(Style::default().fg(TEXT)),
            Rect::new(area.x, y, area.width, 1),
        );
        if area.height >= 3 {
            frame.render_widget(
                Paragraph::new(if empty {
                    "Add a server to get started."
                } else {
                    "Try a name, user or host. Esc clears."
                })
                .alignment(Alignment::Center)
                .style(Style::default().fg(MUTED)),
                Rect::new(area.x, y + 2, area.width, 1),
            );
        }
        return;
    }

    let columns = Columns::new(area);
    let heading_height = 1;
    let row_height = if dense || columns.endpoint.is_some() {
        1
    } else {
        2
    };
    let capacity = (area.height.saturating_sub(heading_height) / row_height) as usize;
    let selected_index = app.selected_index();
    let selected = selected_index
        .and_then(|index| indices.iter().position(|i| *i == index))
        .unwrap_or(0);
    let (start, end) = visible_profile_range(indices.len(), selected, capacity);
    let heading = if end - start < indices.len() {
        format!("{}–{} / {}", start + 1, end, indices.len())
    } else {
        "CONNECTION".to_owned()
    };
    draw_text(frame, columns.name, area.y, &heading, MUTED, false);
    if let Some(endpoint) = columns.endpoint {
        draw_text(frame, columns.auth, area.y, "AUTH", MUTED, false);
        draw_text(frame, endpoint, area.y, "DESTINATION", MUTED, false);
    } else {
        frame.render_widget(
            Paragraph::new("AUTH")
                .alignment(Alignment::Right)
                .style(Style::default().fg(MUTED)),
            Rect::new(columns.auth.x, area.y, columns.auth.width, 1),
        );
    }
    for (row, &index) in indices[start..end].iter().enumerate() {
        let y = area.y + heading_height + row as u16 * row_height;
        let rect = Rect::new(area.x, y, area.width, row_height);
        draw_row(
            frame,
            rect,
            &app.profiles()[index],
            selected_index == Some(index),
        );
        regions.add(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            MouseTarget::Profile(index),
        );
    }
}

fn draw_row(frame: &mut Frame<'_>, area: Rect, profile: &Profile, selected: bool) {
    let columns = Columns::new(area);
    let background = if selected { ACCENT_2 } else { BG };
    // Wide tables use a single row; narrow lists put the destination below
    // the name rather than squeezing three columns into the same line.
    let visual_height = if columns.endpoint.is_some() {
        1
    } else {
        area.height
    };
    frame.render_widget(
        Block::default().style(Style::default().bg(background)),
        Rect::new(area.x, area.y, area.width, visual_height),
    );
    draw_text(
        frame,
        Rect::new(area.x, area.y, 2, 1),
        area.y,
        if selected { "›" } else { " " },
        ACCENT,
        true,
    );
    draw_text(
        frame,
        columns.name,
        area.y,
        &profile.name,
        if selected { ACCENT } else { TEXT },
        selected,
    );
    let endpoint = endpoint(profile);
    if let Some(column) = columns.endpoint {
        draw_text(
            frame,
            column,
            area.y,
            &endpoint,
            if selected { TEXT } else { MUTED },
            false,
        );
    } else if area.height > 1 {
        draw_text(
            frame,
            Rect::new(area.x + 2, area.y, area.width - 3, 1),
            area.y + 1,
            &endpoint,
            MUTED,
            false,
        );
    }
    let auth = match profile.auth {
        AuthMethod::Password { .. } => "PWD",
        AuthMethod::Key { .. } => "KEY",
    };
    if columns.endpoint.is_some() {
        draw_text(
            frame,
            columns.auth,
            area.y,
            auth,
            if selected { ACCENT } else { MUTED },
            selected,
        );
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                auth,
                Style::default().fg(if selected { ACCENT } else { MUTED }),
            )]))
            .alignment(Alignment::Right),
            Rect::new(columns.auth.x, area.y, columns.auth.width, 1),
        );
    }
}

pub(super) fn endpoint(profile: &Profile) -> String {
    if profile.host.contains(':') && !profile.host.starts_with('[') {
        format!("{}@[{}]:{}", profile.username, profile.host, profile.port)
    } else {
        format!("{}@{}:{}", profile.username, profile.host, profile.port)
    }
}

struct Columns {
    name: Rect,
    auth: Rect,
    endpoint: Option<Rect>,
}

impl Columns {
    fn new(area: Rect) -> Self {
        if area.width >= 66 {
            let available = area.width.saturating_sub(10);
            let name_width = ((available as u32 * 42) / 100).clamp(26, 40) as u16;
            let name = Rect::new(area.x + 2, area.y, name_width, 1);
            let auth = Rect::new(name.right() + 2, area.y, 4, 1);
            let endpoint_x = auth.right() + 2;
            let endpoint_width = area.right().saturating_sub(endpoint_x + 1);
            let endpoint = Some(Rect::new(endpoint_x, area.y, endpoint_width, 1));
            Self {
                name,
                auth,
                endpoint,
            }
        } else {
            let auth = Rect::new(area.right().saturating_sub(5), area.y, 4, 1);
            let name_width = auth.x.saturating_sub(area.x + 3);
            let name = Rect::new(area.x + 2, area.y, name_width, 1);
            Self {
                name,
                auth,
                endpoint: None,
            }
        }
    }
}

fn draw_text(
    frame: &mut Frame<'_>,
    column: Rect,
    y: u16,
    text: &str,
    color: ratatui::style::Color,
    bold: bool,
) {
    frame.render_widget(
        Paragraph::new(truncate(text, column.width as usize)).style(
            Style::default().fg(color).add_modifier(if bold {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ),
        Rect::new(column.x, y, column.width, 1),
    );
}

pub(in crate::ui) fn visible_profile_range(
    count: usize,
    selected: usize,
    capacity: usize,
) -> (usize, usize) {
    if count == 0 || capacity == 0 {
        return (0, 0);
    }
    let capacity = capacity.min(count);
    let selected = selected.min(count - 1);
    let start = selected.saturating_sub(capacity / 2).min(count - capacity);
    (start, start + capacity)
}
