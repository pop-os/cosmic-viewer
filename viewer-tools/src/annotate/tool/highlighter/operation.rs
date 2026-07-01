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
    widget::canvas::LineJoin,
};
use image::DynamicImage;
use tiny_skia::LineCap as SkiaLineCap;

/// Highlighter transparency factor
const HIGHLIGHT_ALPHA: f32 = 0.35;

#[derive(Debug, Clone)]
pub struct HighlighterOperation {
    pub points: Vec<Point>,
    pub color: Color,
    pub width: f32,
}

impl HighlighterOperation {
    fn highlight_color(&self) -> Color {
        Color::from_rgba(
            self.color.r,
            self.color.g,
            self.color.b,
            self.color.a * HIGHLIGHT_ALPHA,
        )
    }
}

impl ToolOperation for HighlighterOperation {
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        if self.points.len() < 2 {
            return;
        }

        let path = Path::new(|builder: &mut Builder| {
            builder.move_to(self.points[0]);

            if self.points.len() == 2 {
                builder.line_to(self.points[1]);
            } else {
                let mid = Point::new(
                    f32::midpoint(self.points[0].x, self.points[1].x),
                    f32::midpoint(self.points[0].y, self.points[1].y),
                );

                builder.line_to(mid);

                for idx in 1..self.points.len() - 1 {
                    let control = self.points[idx];
                    let next = self.points[idx + 1];
                    let end = Point::new(
                        f32::midpoint(control.x, next.x),
                        f32::midpoint(control.y, next.y),
                    );

                    builder.quadratic_curve_to(control, end);
                }

                builder.line_to(
                    *self
                        .points
                        .last()
                        .expect("points non-empty: len checked at entry"),
                );
            }
        });

        frame.stroke(
            &path,
            Stroke::default()
                .with_color(self.highlight_color())
                .with_width(self.width * scale)
                .with_line_cap(LineCap::Square)
                .with_line_join(LineJoin::Round),
        );
    }

    fn apply(&self, image: &mut DynamicImage) {
        if self.points.len() < 2 {
            return;
        }

        let Some(path) = build_path(|path_builder| {
            path_builder.move_to(self.points[0].x, self.points[0].y);

            if self.points.len() == 2 {
                path_builder.line_to(self.points[1].x, self.points[1].y);
            } else {
                // Mirror the preview/draw smoothing: line to the first midpoint, then
                // quadratic curves through successive midpoints, then a final segment.
                let mid_x = f32::midpoint(self.points[0].x, self.points[1].x);
                let mid_y = f32::midpoint(self.points[0].y, self.points[1].y);
                path_builder.line_to(mid_x, mid_y);

                for idx in 1..self.points.len() - 1 {
                    let control = self.points[idx];
                    let next = self.points[idx + 1];
                    let end_x = f32::midpoint(control.x, next.x);
                    let end_y = f32::midpoint(control.y, next.y);
                    path_builder.quad_to(control.x, control.y, end_x, end_y);
                }

                let last = *self
                    .points
                    .last()
                    .expect("points non-empty: len checked at entry");
                path_builder.line_to(last.x, last.y);
            }
        }) else {
            return;
        };

        stroke_on_image(
            image,
            &path,
            self.highlight_color(),
            self.width,
            SkiaLineCap::Square,
        );
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
