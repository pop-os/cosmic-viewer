use std::any::Any;

use crate::ToolOperation;
use cosmic::{
    Renderer,
    iced::{Color, Point, Rectangle, Size},
    iced_widget::canvas::{Fill, Frame, Path, Stroke},
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
    fn draw(&self, frame: &mut Frame<Renderer>, image_size: Size, scale: f32) {
        // Show the crop boundary as a subtle dashed outline
        let region = self.region;

        // Dim everything outside of the crop region
        let overlay_color = Color::from_rgba(0.0, 0.0, 0.0, 0.3);
        let frame_size = image_size;

        frame.fill_rectangle(
            Point::ORIGIN,
            Size::new(frame_size.width, region.y),
            Fill::from(overlay_color),
        );

        frame.fill_rectangle(
            Point::new(0.0, region.y + region.height),
            Size::new(
                frame_size.width,
                frame_size.height - region.y - region.height,
            ),
            Fill::from(overlay_color),
        );

        frame.fill_rectangle(
            Point::new(0.0, region.y),
            Size::new(region.x, region.height),
            Fill::from(overlay_color),
        );

        frame.fill_rectangle(
            Point::new(region.x + region.width, region.y),
            Size::new(frame_size.width - region.x - region.width, region.height),
            Fill::from(overlay_color),
        );

        // Thin border to indicate committed crop
        frame.stroke(
            &Path::rectangle(region.position(), region.size()),
            Stroke::default()
                .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.6))
                .with_width(1.0 / scale),
        );
    }

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

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
