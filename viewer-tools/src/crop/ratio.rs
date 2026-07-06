// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::Size;

/// Aspect ratio constraint for the crop tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CropRatio {
    /// Free-form drag
    Custom,
    /// Match the original image aspect ratio.
    Original,
    /// Fixed ratio (always stored as wider:narrower, example: 16:9)
    /// Orientation is derived from the image at usage time.
    Fixed(u32, u32),
}

impl CropRatio {
    /// All preset ratios (stored as landscape). The UI flips them for portrait images.
    #[must_use]
    pub const fn presets() -> &'static [Self] {
        &[
            Self::Custom,
            Self::Original,
            Self::Fixed(1, 1),
            Self::Fixed(16, 9),
            Self::Fixed(7, 5),
            Self::Fixed(4, 3),
            Self::Fixed(3, 2),
        ]
    }

    /// Resolve the ratio as a float (width / height) for the given image size.
    /// For portrait images, fixed ratios are made inverse.
    #[must_use]
    // reason: aspect-ratio components are small presets (e.g. 16, 9); f32 represents them exactly.
    #[allow(clippy::cast_precision_loss)]
    pub fn resolve(&self, image_size: Size) -> Option<f32> {
        let is_portrait = image_size.height > image_size.width;

        match self {
            Self::Custom => None,
            Self::Original => Some(image_size.width / image_size.height),
            Self::Fixed(width, height) => {
                let (width, height) = (*width as f32, *height as f32);
                if is_portrait {
                    Some(height / width)
                } else {
                    Some(width / height)
                }
            }
        }
    }

    /// Display label for the ratio, respecting image orientation.
    #[must_use]
    pub const fn label(&self, is_portrait: bool) -> &'static str {
        match self {
            Self::Custom => "Custom",
            Self::Original => "Original",
            Self::Fixed(width, height) => match (width, height) {
                (1, 1) => "1:1",
                (16, 9) if !is_portrait => "16:9",
                (16, 9) => "9:16",
                (7, 5) if !is_portrait => "7:5",
                (7, 5) => "5:7",
                (4, 3) if !is_portrait => "4:3",
                (4, 3) => "3:4",
                (3, 2) if !is_portrait => "3:2",
                (3, 2) => "2:3",
                _ => "?:?",
            },
        }
    }

    /// Whether this ratio locks the crop frame (fixed-ratio mode).
    #[must_use]
    pub const fn is_constrained(&self) -> bool {
        !matches!(self, Self::Custom)
    }
}
