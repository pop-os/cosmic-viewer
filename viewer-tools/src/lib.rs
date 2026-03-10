pub mod annotate;
pub mod crop;
pub mod renderer;
pub mod rotate;

use crate::rotate::RotateDirection;
use cosmic::{
    Renderer,
    iced::{Point, Rectangle, Size, mouse},
    iced_widget::canvas::Frame,
};
use image::DynamicImage;
use std::{any::Any, fmt::Debug};

/// A tool operation that can be draw as an overlay and applied to an image.
///
/// Committed operations live in the undo/redo stack.
/// Active tool previews (like CropSelection during drag) implement this
/// trait for rendering but are never committed to the stack; they are "transparent"
/// operations.
pub trait ToolOperation: Debug {
    /// Draw the operation's overlay onto the frame.
    /// The frame is already translated/scaled to image coordinates.
    fn draw(&self, frame: &mut Frame<Renderer>, image_size: Size, scale: f32);

    /// Apply the operation destructively to the image pixels.
    /// Called at save time when flattening all committed operations.
    fn apply(&self, image: &mut DynamicImage);

    /// Produce the committed operation from this preview, if applicable.
    /// Returns None if this operation is already committed or has no result.
    fn commit(&self) -> Option<Box<dyn ToolOperation>>;

    /// Downcast support for tool-specific config.
    fn as_any(&self) -> &dyn Any;

    /// Mutable downcast support for tool-specific config.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Called on left mouse press.
    fn on_press(&mut self, point: Point, image_size: Size) -> mouse::Interaction {
        let _ = (point, image_size);
        mouse::Interaction::default()
    }

    /// Called on mouse drag while pressed.
    fn on_drag(&mut self, point: Point, image_size: Size) {
        let _ = (point, image_size);
    }

    /// Called on mouse release.
    fn on_release(&mut self, point: Point, image_size: Size) {
        let _ = (point, image_size);
    }

    /// Returns the cursor to show when hovering at this point.
    fn cursor_at(&self, point: Point) -> mouse::Interaction {
        let _ = point;
        mouse::Interaction::default()
    }

    /// Called when the viewport zoom level changes.
    /// Tools can adjust their coordinates to maintain visual stability.
    fn on_zoom_changed(&mut self, old_zoom: f32, new_zoom: f32, image_size: Size) {
        let _ = (old_zoom, new_zoom, image_size);
    }

    /// Transform this operation's coordinates for a rotation.
    fn transform_rotate(&mut self, _direction: RotateDirection, _image_size: Size) {}

    /// Transform this operation's coordinates for a crop.
    fn transform_crop(&mut self, _region: Rectangle) {}

    /// Returns the bounding box of the operation
    fn bounds(&self) -> Option<Rectangle> {
        None
    }
}
