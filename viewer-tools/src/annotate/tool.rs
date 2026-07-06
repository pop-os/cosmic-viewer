// SPDX-License-Identifier: GPL-3.0-only

pub mod highlighter;
pub mod pen;
pub mod pencil;
pub mod shapes;
pub mod text;

pub use text::FONT_SIZE_PRESETS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnnotateTool {
    #[default]
    Pen,
    Highlighter,
    Text,
    Rectangle,
    Ellipse,
    Arrow,
    Line,
    Star,
    Polygon,
}

impl AnnotateTool {
    /// Presets shown in each dropdown group.
    #[must_use]
    pub const fn draw_tools() -> &'static [Self] {
        &[Self::Pen]
    }

    #[must_use]
    pub const fn shape_tools() -> &'static [Self] {
        &[
            Self::Rectangle,
            Self::Ellipse,
            Self::Arrow,
            Self::Line,
            Self::Star,
            Self::Polygon,
        ]
    }

    #[must_use]
    pub const fn icon_name(&self) -> &'static str {
        match self {
            Self::Pen => "insert-drawing-symbolic",
            Self::Highlighter => "text-highlight-symbolic",
            Self::Text => "insert-text-symbolic",
            Self::Rectangle => "insert-rectangle-symbolic",
            Self::Ellipse => "insert-ellipse-symbolic",
            Self::Arrow => "insert-arrow-symbolic",
            Self::Line => "insert-line-symbolic",
            Self::Star => "insert-star-symbolic",
            Self::Polygon => "insert-polygon-symbolic",
        }
    }
}
