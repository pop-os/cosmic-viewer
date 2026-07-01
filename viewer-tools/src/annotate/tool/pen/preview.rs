use std::any::Any;

use super::PenOperation;
use crate::ToolOperation;
use cosmic::{
    Renderer,
    iced::widget::canvas::{Frame, LineCap, Path, Stroke, path::Builder},
    iced::{Color, Point, Size, mouse},
    widget::canvas::LineJoin,
};
use image::DynamicImage;

#[derive(Debug, Clone)]
pub struct PenPreview {
    pub points: Vec<Point>,
    pub color: Color,
    pub width: f32,
}

impl PenPreview {
    #[must_use]
    pub const fn new(color: Color, width: f32) -> Self {
        Self {
            points: Vec::new(),
            color,
            width,
        }
    }
}

impl ToolOperation for PenPreview {
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        if self.points.len() < 2 {
            return;
        }

        let path = Path::new(|builder: &mut Builder| {
            builder.move_to(self.points[0]);
            for point in &self.points[1..] {
                builder.line_to(*point);
            }
        });

        frame.stroke(
            &path,
            Stroke::default()
                .with_color(self.color)
                .with_width(self.width * scale)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round),
        );
    }

    fn apply(&self, _image: &mut DynamicImage) {
        // Doesn't modify pixels
    }

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        if self.points.len() >= 2 {
            Some(Box::new(PenOperation {
                points: self.points.clone(),
                color: self.color,
                width: self.width,
            }))
        } else {
            None
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn on_press(&mut self, point: Point, _image_size: Size) -> mouse::Interaction {
        self.points.clear();
        self.points.push(point);
        mouse::Interaction::Crosshair
    }

    fn on_drag(&mut self, point: Point, _image_size: Size) {
        self.points.push(point);
    }

    fn on_release(&mut self, _point: Point, _image_size: Size) {
        // Stroke complete
    }
}
