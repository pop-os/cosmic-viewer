mod color;
mod tool;

pub use color::AnnotateColor;
pub use tool::{
    AnnotateTool,
    highlighter::{HighlighterOperation, HighlighterPreview},
    pen::{PenOperation, PenPreview},
    pencil::{PencilOperation, PencilPreview},
    shapes::{ShapeKind, ShapeOperation, ShapePreview},
    text::{TextOperation, TextPreview},
};
