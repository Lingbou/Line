use ratatui::layout::Rect;

use crate::ui::helpers::{centered_fixed, inset};

/// Filtering keeps the launcher stable; resizing or changing the saved
/// collection changes its size. All controls share this geometry.
pub(super) struct BrowseLayout {
    pub(super) shell: Rect,
    pub(super) header: Rect,
    pub(super) search: Rect,
    pub(super) list: Rect,
    pub(super) detail: Rect,
    pub(super) actions: Rect,
    pub(super) footer: Rect,
    pub(super) dense: bool,
}

impl BrowseLayout {
    pub(super) fn resolve(area: Rect, count: usize) -> Self {
        let margin = if area.width >= 80 { 4 } else { 0 };
        let width = area.width.saturating_sub(margin).min(104);
        let narrow = width.saturating_sub(4) < 66;
        let row_height = if narrow { 2 } else { 1 };
        let chrome_height = if narrow { 11 } else { 13 };
        let height = (count.min(24) as u16 * row_height + chrome_height).clamp(16, 32);
        let shell = centered_fixed(area, width, height);
        let inner = inset(shell, 2, 1);
        let dense = shell.height < 16;
        let header_height = if dense || narrow { 1 } else { 2 };
        let search_height = if dense { 1 } else { 2 };
        let detail_height = if dense || narrow { 1 } else { 3 };
        let actions_height = if narrow { 2 } else { 1 };
        let footer_height = if dense { 1 } else { 2 };
        let header = Rect::new(inner.x, inner.y, inner.width, header_height);
        let search = Rect::new(inner.x, header.bottom(), inner.width, search_height);
        let footer = Rect::new(
            inner.x,
            inner.bottom() - footer_height,
            inner.width,
            footer_height,
        );
        let actions = Rect::new(
            inner.x,
            footer.y - actions_height,
            inner.width,
            actions_height,
        );
        let detail = Rect::new(
            inner.x,
            actions.y - detail_height,
            inner.width,
            detail_height,
        );
        let list = Rect::new(
            inner.x,
            search.bottom(),
            inner.width,
            detail.y - search.bottom(),
        );
        Self {
            shell,
            header,
            search,
            list,
            detail,
            actions,
            footer,
            dense,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_sizes_keep_rows_and_actions_inside_the_shell() {
        for (width, height) in [(40, 12), (40, 22), (80, 24), (120, 34), (240, 64)] {
            for count in [0, 1, 4, 100] {
                let layout = BrowseLayout::resolve(Rect::new(3, 5, width, height), count);
                assert!(layout.shell.width <= 120);
                assert!(layout.shell.height <= 32);
                assert!(layout.list.height >= 2);
                let mut previous_bottom = layout.shell.y;
                for rect in [
                    layout.header,
                    layout.search,
                    layout.list,
                    layout.detail,
                    layout.actions,
                    layout.footer,
                ] {
                    assert!(rect.y >= previous_bottom);
                    assert!(rect.x >= layout.shell.x);
                    assert!(rect.right() <= layout.shell.right());
                    assert!(rect.bottom() < layout.shell.bottom());
                    previous_bottom = rect.bottom();
                }
            }
        }
    }
}
