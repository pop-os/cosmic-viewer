// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::mouse;

/// Which part of the crop selection the user is dragging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragHandle {
    /// Not dragging any handle.
    None,
    // Corners
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    // Edges
    Top,
    Bottom,
    Left,
    Right,
    // Interior
    Move,
}

impl DragHandle {
    #[must_use]
    pub const fn cursor(&self) -> mouse::Interaction {
        match self {
            Self::None => mouse::Interaction::Crosshair,
            Self::TopLeft | Self::BottomRight => mouse::Interaction::ResizingDiagonallyDown,
            Self::TopRight | Self::BottomLeft => mouse::Interaction::ResizingDiagonallyUp,
            Self::Top | Self::Bottom => mouse::Interaction::ResizingVertically,
            Self::Left | Self::Right => mouse::Interaction::ResizingHorizontally,
            Self::Move => mouse::Interaction::Grabbing,
        }
    }
}
