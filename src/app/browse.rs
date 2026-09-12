use unicode_segmentation::UnicodeSegmentation;

use super::{App, helpers::prev_word_boundary};

impl App {
    /// Live search text entered on the connection launcher.
    pub fn browse_query(&self) -> &str {
        &self.browse_query
    }

    /// Matching indices into [`Self::profiles`], kept in their saved order.
    /// The renderer and mouse targets use these actual indices, not row numbers.
    pub fn visible_profile_indices(&self) -> &[usize] {
        &self.visible_profiles
    }

    pub(super) fn append_browse_query(&mut self, text: &str) {
        self.browse_query
            .extend(text.chars().filter(|character| !character.is_control()));
        self.refresh_browse_matches();
        self.status_message = None;
    }

    pub(super) fn backspace_browse_query(&mut self) {
        if let Some((start, _)) = self.browse_query.grapheme_indices(true).next_back() {
            self.browse_query.truncate(start);
            self.refresh_browse_matches();
        }
    }

    pub(super) fn backspace_word_browse_query(&mut self) {
        if self.browse_query.is_empty() {
            return;
        }
        let len = self.browse_query.chars().count();
        let target = prev_word_boundary(&self.browse_query, len);
        let byte_index = self
            .browse_query
            .char_indices()
            .nth(target)
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.browse_query.truncate(byte_index);
        self.refresh_browse_matches();
    }

    pub(super) fn clear_browse_query(&mut self) {
        if !self.browse_query.is_empty() {
            self.browse_query.clear();
            self.refresh_browse_matches();
        }
    }

    /// Recompute only after query/profile changes, never on a redraw.
    pub(super) fn refresh_browse_matches(&mut self) {
        let query = self.browse_query.to_lowercase();
        self.visible_profiles.clear();
        self.visible_profiles.extend(
            self.profiles
                .iter()
                .enumerate()
                .filter(|(_, profile)| {
                    query.is_empty()
                        || [&profile.name, &profile.username, &profile.host]
                            .iter()
                            .any(|value| value.to_lowercase().contains(&query))
                })
                .map(|(index, _)| index),
        );
        if !self.visible_profiles.contains(&self.selected) {
            self.selected = self.visible_profiles.first().copied().unwrap_or(0);
        }
    }
}
