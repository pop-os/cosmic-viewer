pub mod crop;

use cosmic::{Renderer, iced::Size, iced_widget::canvas::Frame};
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
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
