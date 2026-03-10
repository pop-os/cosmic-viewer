use std::any::Any;

use super::PencilOperation;
use crate::ToolOperation;
use cosmic::{
    Renderer,
    iced::{Color, Point, Size, mouse},
    iced_widget::canvas::{Frame, LineCap, Path, Stroke, path::Builder},
};
use image::DynamicImage;

#[derive(Debug, Clone)]
pub struct PencilPreview {
    pub points: Vec<Point>,
    pub color: Color,
    pub width: f32,
}

impl PencilPreview {
    pub fn new(color: Color, width: f32) -> Self {
        Self {
            points: Vec::new(),
            color,
            width,
        }
    }
}

impl ToolOperation for PencilPreview {
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

        let mut pencil_color = self.color;
        pencil_color.a *= 0.85;

        frame.stroke(
            &path,
            Stroke::default()
                .with_color(pencil_color)
                .with_width(self.width / scale)
                .with_line_cap(LineCap::Butt),
        );
    }

    fn apply(&self, _image: &mut DynamicImage) {}

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        if self.points.len() >= 2 {
            Some(Box::new(PencilOperation {
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
        self.points.push(point)
    }

    fn on_release(&mut self, _point: Point, _image_size: Size) {}
}
