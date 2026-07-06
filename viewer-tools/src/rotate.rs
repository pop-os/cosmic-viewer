// SPDX-License-Identifier: GPL-3.0-only

use crate::ToolOperation;
use cosmic::{Renderer, iced::Size, iced::widget::canvas::Frame};
use image::DynamicImage;
use std::any::Any;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotateDirection {
    Left,
    Right,
}

impl RotateDirection {
    #[must_use]
    pub const fn inverse(&self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RotateOperation {
    pub direction: RotateDirection,
}

impl RotateOperation {
    #[must_use]
    pub const fn new(direction: RotateDirection) -> Self {
        Self { direction }
    }
}

impl ToolOperation for RotateOperation {
    fn draw(&self, _frame: &mut Frame<Renderer>, _image_size: Size, _scale: f32) {
        // No overlay for rotation
    }

    fn apply(&self, image: &mut DynamicImage) {
        *image = match self.direction {
            RotateDirection::Left => image.rotate270(),
            RotateDirection::Right => image.rotate90(),
        };
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
