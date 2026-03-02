use crate::ToolOperation;
use cosmic::{
    Renderer,
    iced::{Color, Point, Size},
    iced_widget::canvas::{Frame, LineCap, Path, Stroke, path::Builder},
    widget::canvas::LineJoin,
};
use image::{DynamicImage, Rgba};
use imageproc::{drawing::draw_antialiased_line_segment_mut, pixelops::interpolate};

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
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, _scale: f32) {
        if self.points.len() < 2 {
            return;
        }

        let path = Path::new(|builder: &mut Builder| {
            builder.move_to(self.points[0]);

            if self.points.len() == 2 {
                builder.line_to(self.points[1]);
            } else {
                let mid = Point::new(
                    (self.points[0].x + self.points[1].x) / 2.0,
                    (self.points[0].y + self.points[1].y) / 2.0,
                );

                builder.line_to(mid);

                for idx in 1..self.points.len() - 1 {
                    let control = self.points[idx];
                    let next = self.points[idx + 1];
                    let end = Point::new((control.x + next.x) / 2.0, (control.y + next.y) / 2.0);

                    builder.quadratic_curve_to(control, end);
                }

                builder.line_to(*self.points.last().unwrap());
            }
        });

        frame.stroke(
            &path,
            Stroke::default()
                .with_color(self.highlight_color())
                .with_width(self.width)
                .with_line_cap(LineCap::Square)
                .with_line_join(LineJoin::Round),
        );
    }

    fn apply(&self, image: &mut DynamicImage) {
        if self.points.len() < 2 {
            return;
        }

        let rgba_img = image.as_mut_rgba8().expect("image should be RGBA");
        let alpha = self.color.a * HIGHLIGHT_ALPHA;
        let color = Rgba([
            (self.color.r * 255.0) as u8,
            (self.color.g * 255.0) as u8,
            (self.color.b * 255.0) as u8,
            (alpha * 255.0) as u8,
        ]);

        for pair in self.points.windows(2) {
            let start = (pair[0].x as i32, pair[0].y as i32);
            let end = (pair[1].x as i32, pair[1].y as i32);
            draw_antialiased_line_segment_mut(rgba_img, start, end, color, interpolate);
        }

        *image = DynamicImage::ImageRgba8(rgba_img.clone());
    }

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        None
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
