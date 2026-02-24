use cosmic::iced::Size;

/// Aspect ratio constraint for the crop tool.
#[derive(Debug, Clone, Copy, PartialEq)]
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
    pub fn presets() -> &'static [CropRatio] {
        &[
            CropRatio::Custom,
            CropRatio::Original,
            CropRatio::Fixed(1, 1),
            CropRatio::Fixed(16, 9),
            CropRatio::Fixed(7, 5),
            CropRatio::Fixed(4, 3),
            CropRatio::Fixed(3, 2),
        ]
    }

    /// Resolve the ratio as a float (width / height) for the given image size.
    /// For portrait images, fixed ratios are made inverse.
    pub fn resolve(&self, image_size: Size) -> Option<f32> {
        let is_portrait = image_size.height > image_size.width;

        match self {
            CropRatio::Custom => None,
            CropRatio::Original => Some(image_size.width / image_size.height),
            CropRatio::Fixed(width, height) => {
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
    pub fn label(&self, is_portrait: bool) -> &'static str {
        match self {
            /* CropRatio::Custom => fl!("crop-custom"),
            CropRatio::Original => fl!("crop-original"),
            CropRatio::Fixed(width, height) => match (width, height) {
                (1, 1) => fl!("crop-1-1"),
                (16, 9) if !is_portrait => fl!("crop-16-9"),
                (16, 9) => fl!("crop-9-16"),
                (7, 5) if !is_portrait => fl!("crop-7-5"),
                (7, 5) => fl!("crop-5-7"),
                (4, 3) if !is_portrait => fl!("crop-4-3"),
                (4, 3) => fl!("crop-3-4"),
                (3, 2) if !is_portrait => fl!("crop-3-2"),
                (3, 2) => fl!("crop-2-3"), */
            CropRatio::Custom => "Custom",
            CropRatio::Original => "Original",
            CropRatio::Fixed(width, height) => match (width, height) {
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
    pub fn is_constrained(&self) -> bool {
        !matches!(self, CropRatio::Custom)
    }
}
