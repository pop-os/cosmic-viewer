mod color;
mod tool;

pub use color::AnnotateColor;
pub use tool::{
    AnnotateTool, FONT_SIZE_PRESETS,
    highlighter::{HighlighterOperation, HighlighterPreview},
    pen::{PenOperation, PenPreview},
    pencil::{PencilOperation, PencilPreview},
    shapes::{ShapeKind, ShapeOperation, ShapePreview},
    text::{TextDragHandle, TextOperation, TextPreview},
};
