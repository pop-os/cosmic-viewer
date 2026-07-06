// SPDX-License-Identifier: GPL-3.0-only

use std::any::Any;

use super::HighlighterOperation;
use crate::ToolOperation;
use cosmic::{
    Renderer,
    iced::widget::canvas::{Frame, LineCap, Path, Stroke, path::Builder},
    iced::{Color, Point, Size, mouse},
    widget::canvas::LineJoin,
};
use image::DynamicImage;

/// Highlighter transparency factor
const HIGHLIGHT_ALPHA: f32 = 0.35;

#[derive(Debug, Clone)]
pub struct HighlighterPreview {
    pub points: Vec<Point>,
    pub color: Color,
    pub width: f32,
}

impl HighlighterPreview {
    #[must_use]
    pub const fn new(color: Color, width: f32) -> Self {
        Self {
            points: Vec::new(),
            color,
            width,
        }
    }

    fn highlight_color(&self) -> Color {
        Color::from_rgba(
            self.color.r,
            self.color.g,
            self.color.b,
            self.color.a * HIGHLIGHT_ALPHA,
        )
    }
}

impl ToolOperation for HighlighterPreview {
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        if self.points.len() < 2 {
            return;
        }

        let path = Path::new(|builder: &mut Builder| {
            builder.move_to(self.points[0]);

            if self.points.len() == 2 {
                builder.line_to(self.points[1]);
            } else {
                // Line to midpoint of first two points
                let mid = Point::new(
                    f32::midpoint(self.points[0].x, self.points[1].x),
                    f32::midpoint(self.points[0].y, self.points[1].y),
                );
                builder.line_to(mid);

                // Quadratic curves through successive midpoints
                for idx in 1..self.points.len() - 1 {
                    let control = self.points[idx];
                    let next = self.points[idx + 1];
                    let end = Point::new(
                        f32::midpoint(control.x, next.x),
                        f32::midpoint(control.y, next.y),
                    );
                    builder.quadratic_curve_to(control, end);
                }

                // Final segment to last point
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

    fn apply(&self, _image: &mut DynamicImage) {
        // Never modifies pixels
    }

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        if self.points.len() >= 2 {
            Some(Box::new(HighlighterOperation {
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
        if let Some(last) = self.points.last() {
            let dx = point.x - last.x;
            let dy = point.y - last.y;
            let min_dist = self.width * 0.5;

            if dy.mul_add(dy, dx * dx) < min_dist * min_dist {
                // Skip to reduce join overlap
                return;
            }
        }
        self.points.push(point);
    }

    fn on_release(&mut self, _point: Point, _image_size: Size) {
        // Points already captured during drag; nothing to finalize.
    }
}
