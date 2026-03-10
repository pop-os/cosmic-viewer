use std::any::Any;

use crate::ToolOperation;
use cosmic::{
    Renderer,
    iced::{Rectangle, Size},
    iced_widget::canvas::Frame,
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
    pub fn new(region: Rectangle) -> Self {
        Self { region }
    }
}

impl ToolOperation for CropOperation {
    fn draw(&self, _frame: &mut Frame<Renderer>, _image_size: Size, _scale: f32) {}

    fn apply(&self, image: &mut DynamicImage) {
        let region = self.region;
        *image = image.crop_imm(
            region.x as u32,
            region.y as u32,
            region.width as u32,
            region.height as u32,
        );
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
