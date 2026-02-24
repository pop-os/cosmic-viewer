pub mod crop;

use cosmic::{Renderer, iced_widget::canvas::Frame};
use image::DynamicImage;
use std::fmt::Debug;

/// A tool operation that can be draw as an overlay and applied to an image.
///
/// Committed operations live in the undo/redo stack.
/// Active tool previews (like CropSelection during drag) implement this
/// trait for rendering but are never committed to the stack; they are "transparent"
/// operations.
pub trait ToolOperation: Debug {
    /// Draw the operation's overlay onto the frame.
    /// The frame is already translated/scaled to image coordinates.
    fn draw(&self, frame: &mut Frame<Renderer>);

    /// Apply the operation destructively to the image pixels.
    /// Called at save time when flattening all committed operations.
    fn apply(&self, image: &mut DynamicImage);
}
