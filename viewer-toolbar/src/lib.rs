mod toolbar;
mod toolbar_item;

pub use toolbar::{ResponsiveToolbar, responsive_toolbar};
pub use toolbar_item::ToolbarItem;

/// Responsive toolbar breakpoints
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarMode {
    /// Full toolbar - all items visible
    Full,
    /// Compact - some items in overflow menu
    Compact,
    /// Minimal - most items in overflow menu
    Minimal,
}

impl ToolbarMode {
    /// Determine toolbar mode based on available width
    #[must_use]
    pub fn from_width(width: f32) -> Self {
        if width >= 600.0 {
            Self::Full
        } else if width >= 400.0 {
            Self::Compact
        } else {
            Self::Minimal
        }
    }
}

/// Item priority for overflow handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ItemPriority {
    /// Always visible
    Essential,
    /// Visible in full and compact
    Standard,
    /// Only visible in full
    Optional,
}
