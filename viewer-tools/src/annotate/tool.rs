pub mod highlighter;
pub mod pen;
pub mod pencil;
pub mod shapes;
pub mod text;

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
    /// Return the Icon name for each tool

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
            Self::Pen => "pen-symbolic",
            Self::Pencil => "pencil-symbolic",
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
