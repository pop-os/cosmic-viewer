use std::any::Any;

use super::{
    ShapeKind, arrow_segments, draw_shape, normalize_rect, polygon_vertices, star_vertices,
};
use crate::{
    ToolOperation,
    renderer::{build_path, stroke_on_image},
    rotate::RotateDirection,
};
use cosmic::{
    Renderer,
    iced::{Color, Point, Rectangle, Size},
    iced_widget::canvas::Frame,
};
use image::DynamicImage;
use tiny_skia::{LineCap as SkiaLineCap, Rect};

#[derive(Debug, Clone)]
pub struct ShapeOperation {
    pub kind: ShapeKind,
    pub start: Point,
    pub end: Point,
    pub color: Color,
    pub width: f32,
}

impl ShapeOperation {
    pub fn new(kind: ShapeKind, start: Point, end: Point, color: Color, width: f32) -> Self {
        Self {
            kind,
            start,
            end,
            color,
            width,
        }
    }
}

impl ToolOperation for ShapeOperation {
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        draw_shape(
            self.kind, self.start, self.end, self.color, self.width, frame, scale,
        );
    }

    fn apply(&self, image: &mut DynamicImage) {
        let Some(path) = build_path(|path_builder| match self.kind {
            ShapeKind::Rectangle => {
                let rect = normalize_rect(self.start, self.end);
                if let Some(rect) = Rect::from_xywh(rect.x, rect.y, rect.width, rect.height) {
                    path_builder.push_rect(rect);
                }
            }
            ShapeKind::Ellipse => {
                let rect = normalize_rect(self.start, self.end);
                path_builder.push_oval(
                    Rect::from_xywh(rect.x, rect.y, rect.width, rect.height)
                        .unwrap_or(Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap()),
                );
            }
            ShapeKind::Line => {
                path_builder.move_to(self.start.x, self.start.y);
                path_builder.line_to(self.end.x, self.end.y);
            }
            ShapeKind::Arrow => {
                for (a, b) in arrow_segments(self.start, self.end) {
                    path_builder.move_to(a.x, a.y);
                    path_builder.line_to(b.x, b.y);
                }
            }
            ShapeKind::Star => {
                let verts = star_vertices(self.start, self.end);
                if let Some(first) = verts.first() {
                    path_builder.move_to(first.x, first.y);
                    for vert in &verts[1..] {
                        path_builder.line_to(vert.x, vert.y);
                    }
                    path_builder.close();
                }
            }
            ShapeKind::Polygon => {
                let verts = polygon_vertices(self.start, self.end, 6);
                if let Some(first) = verts.first() {
                    path_builder.move_to(first.x, first.y);
                    for vert in &verts[1..] {
                        path_builder.line_to(vert.x, vert.y);
                    }
                    path_builder.close();
                }
            }
        }) else {
            return;
        };

        stroke_on_image(image, &path, self.color, self.width, SkiaLineCap::Round);
    }

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        None // Already committed
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn transform_rotate(&mut self, direction: RotateDirection, image_size: Size) {
        let (width, height) = (image_size.width, image_size.height);
        for point in [&mut self.start, &mut self.end] {
            let (x, y) = (point.x, point.y);

            *point = match direction {
                RotateDirection::Left => Point::new(y, width - x),
                RotateDirection::Right => Point::new(height - y, x),
            }
        }
    }

    fn transform_crop(&mut self, region: Rectangle) {
        for point in [&mut self.start, &mut self.end] {
            point.x -= region.x;
            point.y -= region.y;
        }
    }
}
