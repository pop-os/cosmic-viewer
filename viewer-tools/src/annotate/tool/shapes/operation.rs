use std::any::Any;

use super::{
    ShapeKind, arrow_segments, draw_shape, normalize_rect, polygon_vertices, star_vertices,
};
use crate::{
    ToolOperation,
    renderer::{build_path, fill_on_image, stroke_on_image},
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

impl ShapeOperation {
    pub fn bounds(&self) -> Rectangle {
        normalize_rect(self.start, self.end)
    }

    pub fn hit_test(&self, point: Point) -> bool {
        let b = self.bounds();
        let pad = self.width.max(4.0);
        Rectangle::new(
            Point::new(b.x - pad, b.y - pad),
            Size::new(b.width + pad * 2.0, b.height + pad * 2.0),
        )
        .contains(point)
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.start.x += dx;
        self.start.y += dy;
        self.end.x += dx;
        self.end.y += dy;
    }
}

impl ToolOperation for ShapeOperation {
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        draw_shape(
            self.kind, self.start, self.end, self.color, self.width, frame, scale,
        );
    }

    fn apply(&self, image: &mut DynamicImage) {
        match self.kind {
            ShapeKind::Star | ShapeKind::Polygon => {
                let Some(path) = build_path(|pb| {
                    let verts = match self.kind {
                        ShapeKind::Star => star_vertices(self.start, self.end),
                        ShapeKind::Polygon => polygon_vertices(self.start, self.end, 6),
                        _ => unreachable!(),
                    };
                    if let Some(first) = verts.first() {
                        pb.move_to(first.x, first.y);
                        for vert in &verts[1..] {
                            pb.line_to(vert.x, vert.y);
                        }
                        pb.close();
                    }
                }) else {
                    return;
                };
                fill_on_image(image, &path, self.color);
            }
            ShapeKind::Arrow => {
                if let Some(shaft) = build_path(|pb| {
                    pb.move_to(self.start.x, self.start.y);
                    pb.line_to(self.end.x, self.end.y);
                }) {
                    stroke_on_image(image, &shaft, self.color, self.width, SkiaLineCap::Round);
                }
                let segs = arrow_segments(self.start, self.end);
                if segs.len() >= 3 {
                    let tip = segs[1].1;
                    let left = segs[1].0;
                    let right = segs[2].0;
                    if let Some(head) = build_path(|pb| {
                        pb.move_to(tip.x, tip.y);
                        pb.line_to(left.x, left.y);
                        pb.line_to(right.x, right.y);
                        pb.close();
                    }) {
                        fill_on_image(image, &head, self.color);
                    }
                }
            }
            _ => {
                let Some(path) = build_path(|pb| match self.kind {
                    ShapeKind::Rectangle => {
                        let rect = normalize_rect(self.start, self.end);
                        if let Some(rect) =
                            Rect::from_xywh(rect.x, rect.y, rect.width, rect.height)
                        {
                            pb.push_rect(rect);
                        }
                    }
                    ShapeKind::Ellipse => {
                        let rect = normalize_rect(self.start, self.end);
                        pb.push_oval(
                            Rect::from_xywh(rect.x, rect.y, rect.width, rect.height)
                                .unwrap_or(Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap()),
                        );
                    }
                    ShapeKind::Line => {
                        pb.move_to(self.start.x, self.start.y);
                        pb.line_to(self.end.x, self.end.y);
                    }
                    _ => unreachable!(),
                }) else {
                    return;
                };
                stroke_on_image(image, &path, self.color, self.width, SkiaLineCap::Round);
            }
        }
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

    fn movable(&self) -> bool {
        true
    }

    fn hit_test(&self, point: Point) -> bool {
        ShapeOperation::hit_test(self, point)
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        ShapeOperation::translate(self, dx, dy);
    }

    fn bounds(&self) -> Option<Rectangle> {
        Some(ShapeOperation::bounds(self))
    }
}
