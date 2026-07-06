// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    iced::{Point, Vector},
    widget::image::Handle,
};
use std::fmt::{self, Debug, Formatter};

/// An image to display on the canvas.
///
/// Represents the working copy - either the original from cache
/// or a preview with base transforms applied (rotate, flip).
/// Tools render overlays on top; pixels aren't modified until save.
#[derive(Clone)]
pub struct CanvasImage {
    pub handle: Handle,
    pub width: u32,
    pub height: u32,
}

impl Debug for CanvasImage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("CanvasImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// Messages emitted by the canvas to the application
#[derive(Debug, Clone)]
pub enum CanvasMessage {
    /// Right-click context menu. Some(point) opens at position, None closes.
    ContextMenu(Option<Point>),
    ZoomIn,
    ZoomOut,
    ZoomBy(f32),
    Pan(Vector),
    ActualSize,
    FitToView,
    Fullscreen,
    ToolStart(Point),
    ToolDrag(Point),
    ToolEnd,
}

/// Active tool on the canvas
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Crop,
    Rotate,
    Annotate,
    Pan,
}

/// Widget-internal interaction state machine.
#[derive(Debug, Default)]
pub enum Interaction {
    #[default]
    None,
    /// Dragging to pan the viewport.
    Panning { start: Point, start_pan: Vector },
}
