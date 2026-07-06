// SPDX-License-Identifier: GPL-3.0-only

use std::any::Any;

use crate::{
    ToolOperation,
    renderer::{build_path, stroke_on_image},
    rotate::RotateDirection,
};
use cosmic::{
    Renderer,
    iced::widget::canvas::{Frame, LineCap, Path, Stroke, path::Builder},
    iced::{Color, Point, Rectangle, Size},
};
use image::DynamicImage;
use tiny_skia::LineCap as SkiaLineCap;

#[derive(Debug, Clone)]
pub struct PencilOperation {
    pub points: Vec<Point>,
    pub color: Color,
    pub width: f32,
}

impl ToolOperation for PencilOperation {
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, _scale: f32) {
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
                .with_width(self.width)
                .with_line_cap(LineCap::Butt),
        );
    }

    fn apply(&self, image: &mut DynamicImage) {
        if self.points.len() < 2 {
            return;
        }

        let Some(path) = build_path(|path_builder| {
            path_builder.move_to(self.points[0].x, self.points[0].y);
            for point in &self.points[1..] {
                path_builder.line_to(point.x, point.y);
            }
        }) else {
            return;
        };

        let mut color = self.color;
        color.a *= 0.85;

        stroke_on_image(image, &path, color, self.width, SkiaLineCap::Butt);
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

    fn transform_rotate(&mut self, direction: RotateDirection, image_size: Size) {
        let (width, height) = (image_size.width, image_size.height);
        for point in &mut self.points {
            let (x, y) = (point.x, point.y);

            *point = match direction {
                RotateDirection::Left => Point::new(y, width - x),
                RotateDirection::Right => Point::new(height - y, x),
            }
        }
    }

    fn transform_crop(&mut self, region: Rectangle) {
        for point in &mut self.points {
            point.x -= region.x;
            point.y -= region.y;
        }
    }
}
