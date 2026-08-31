use super::FormField;

/// A target rectangle used by the renderer for mouse hit testing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseTarget {
    Tab(usize),
    Connect,
    Add,
    Edit,
    Delete,
    Save,
    Cancel,
    Confirm,
    Dismiss,
    Field(FormField),
    AuthPassword,
    AuthKey,
    KeyImport,
    KeyExisting,
    KeyPaste,
    DeleteKey,
    ShowPassword,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MouseRegion {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    target: MouseTarget,
}

/// Hit boxes populated by [`crate::ui::draw`]. Keeping this in the app state
/// lets mouse support remain deterministic and independent of terminal size.
#[derive(Clone, Debug, Default)]
pub struct MouseRegions {
    regions: Vec<MouseRegion>,
}

impl MouseRegions {
    pub fn add(&mut self, x: u16, y: u16, width: u16, height: u16, target: MouseTarget) {
        if width > 0 && height > 0 {
            self.regions.push(MouseRegion {
                x,
                y,
                width,
                height,
                target,
            });
        }
    }

    pub(super) fn target_at(&self, x: u16, y: u16) -> Option<MouseTarget> {
        self.regions
            .iter()
            .rev()
            .find(|region| {
                x >= region.x
                    && x < region.x.saturating_add(region.width)
                    && y >= region.y
                    && y < region.y.saturating_add(region.height)
            })
            .map(|region| region.target)
    }
}
