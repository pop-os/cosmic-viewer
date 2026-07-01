use std::any::Any;

use crate::ToolOperation;
use cosmic::{
    Renderer,
    iced::widget::canvas::Frame,
    iced::{Rectangle, Size},
};
use image::DynamicImage;

/// A committed crop operation. Stored on the undo/redo stack.
/// When flattened at save time, applies the crop to the image.
#[derive(Debug, Clone)]
pub struct CropOperation {
    /// The crop region in pixels
    pub region: Rectangle,
}

impl CropOperation {
    #[must_use]
    pub const fn new(region: Rectangle) -> Self {
        Self { region }
    }
}

impl ToolOperation for CropOperation {
    fn draw(&self, _frame: &mut Frame<Renderer>, _image_size: Size, _scale: f32) {}

    // reason: region holds non-negative pixel coordinates within image bounds;
    // rounding to the nearest pixel and saturating (float->int saturates in Rust)
    // is the intended quantization, and crop_imm clamps any overshoot.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn apply(&self, image: &mut DynamicImage) {
        // Re-validate and clamp the stored region against the *actual* image this op is being
        // replayed onto (it may differ from the image the region was captured against). Without
        // this, a region landing outside the bounds collapses crop_imm to a silently-written 0x0
        // image (total data loss). round() also removes the sub-pixel truncation of `as u32`.
        let (iw, ih) = (image.width(), image.height());
        let x = (self.region.x.max(0.0).round() as u32).min(iw);
        let y = (self.region.y.max(0.0).round() as u32).min(ih);
        let w = (self.region.width.max(0.0).round() as u32).min(iw.saturating_sub(x));
        let h = (self.region.height.max(0.0).round() as u32).min(ih.saturating_sub(y));
        if w == 0 || h == 0 {
            // Degenerate region: leave the image unchanged rather than zeroing it.
            return;
        }
        *image = image.crop_imm(x, y, w, h);
    }

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
