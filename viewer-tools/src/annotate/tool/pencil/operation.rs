use crate::ToolOperation;
use cosmic::{
    Renderer,
    iced::{Color, Point, Size},
    iced_widget::canvas::{Frame, LineCap, Path, Stroke, path::Builder},
};
use image::{DynamicImage, Rgba};
use imageproc::{drawing::draw_antialiased_line_segment_mut, pixelops::interpolate};

#[derive(Debug, Clone)]
pub struct PencilOperation {
    pub points: Vec<Point>,
    pub color: Color,
    pub width: f32,
}

impl ToolOperation for PencilOperation {
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

    fn apply(&self, image: &mut DynamicImage) {
        if self.points.len() < 2 {
            return;
        }

        let rgba = image.as_mut_rgba8().expect("image should be RGBA");
        let color = Rgba([
            (self.color.r * 255.0) as u8,
            (self.color.g * 255.0) as u8,
            (self.color.b * 255.0) as u8,
            (self.color.a * 0.85 * 255.0) as u8,
        ]);

        for pair in self.points.windows(2) {
            let start = (pair[0].x as i32, pair[1].x as i32);
            let end = (pair[0].y as i32, pair[1].y as i32);
            draw_antialiased_line_segment_mut(rgba, start, end, color, interpolate);
        }

        *image = DynamicImage::ImageRgba8(rgba.clone());
    }

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        None
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
