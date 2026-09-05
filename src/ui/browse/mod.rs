mod chrome;
mod detail;
mod layout;
mod list;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Block, BorderType, Borders},
};

use crate::app::{App, MouseRegions};
use crate::ui::theme::{BG, BORDER};

use self::{
    chrome::{draw_actions, draw_footer, draw_header, draw_search},
    detail::draw_detail,
    layout::BrowseLayout,
    list::draw_connection_list,
};

pub(super) fn draw_browse(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    regions: &mut MouseRegions,
) {
    let layout = BrowseLayout::resolve(area, app.profiles().len());
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(BG)),
        layout.shell,
    );
    draw_header(frame, layout.header, app, regions);
    draw_search(frame, layout.search, app);
    draw_connection_list(frame, layout.list, app, regions, layout.dense);
    draw_detail(frame, layout.detail, app, layout.dense);
    draw_actions(frame, layout.actions, app, regions);
    draw_footer(frame, layout.footer, app);
}

#[cfg(test)]
pub(super) use list::visible_profile_range;
