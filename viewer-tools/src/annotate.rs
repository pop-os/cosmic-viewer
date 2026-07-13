// SPDX-License-Identifier: GPL-3.0-only

mod color;
pub mod tool;

pub use color::AnnotateColor;
pub use tool::{
    AnnotateTool, FONT_SIZE_LABELS, FONT_SIZE_PRESETS_PT,
    highlighter::{HighlighterOperation, HighlighterPreview},
    pen::{PenOperation, PenPreview},
    pencil::{PencilOperation, PencilPreview},
    pt_to_px,
    shapes::{ShapeKind, ShapeOperation, ShapePreview},
    text::{TextDragHandle, TextOperation, TextPreview},
};
