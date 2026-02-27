pub mod highlighter;
pub mod pen;
pub mod pencil;
pub mod shapes;
pub mod text;

pub use pen::{PenOperation, PenPreview};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnnotateTool {
    #[default]
    Pen,
    Pencil,
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
    pub fn draw_tools() -> &'static [AnnotateTool] {
        &[AnnotateTool::Pen, AnnotateTool::Pencil]
    }

    pub fn shape_tools() -> &'static [AnnotateTool] {
        &[
            AnnotateTool::Rectangle,
            AnnotateTool::Ellipse,
            AnnotateTool::Arrow,
            AnnotateTool::Line,
            AnnotateTool::Star,
            AnnotateTool::Polygon,
        ]
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            Self::Pen => "edit-symbolic",
            Self::Pencil => "edit-symbolic",
            Self::Highlighter => "format-text-underline-symbolic",
            Self::Text => "format-text-bold-symbolic",
            Self::Rectangle => "insert-object-symbolic",
            Self::Ellipse => "insert-object-symbolic",
            Self::Arrow => "insert-object-symbolic",
            Self::Line => "insert-object-symbolic",
            Self::Star => "insert-object-symbolic",
            Self::Polygon => "insert-object-symbolic",
        }
    }
}
