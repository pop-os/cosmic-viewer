// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::Color;

/// Preset annotation colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnnotateColor(pub Color);

impl AnnotateColor {
    #[must_use]
    pub fn presets() -> Vec<Self> {
        vec![
            Self(Color::WHITE),
            Self(Color::from_rgb(1.0, 0.0, 0.0)),  // Red
            Self(Color::from_rgb(1.0, 0.65, 0.0)), // Orange
            Self(Color::from_rgb(0.0, 1.0, 0.0)),  // Green
            Self(Color::from_rgb(0.0, 0.0, 1.0)),  // Blue
            Self(Color::BLACK),
        ]
    }
}

impl Default for AnnotateColor {
    fn default() -> Self {
        Self(Color::BLACK)
    }
}
