use super::{
    ShapeKind, arrow_segments, closed_segments, draw_shape, ellipse_segments, normalize_rect,
    polygon_vertices, star_vertices,
};
use crate::ToolOperation;
use cosmic::{
    Renderer,
    iced::{Color, Point, Size},
    iced_widget::canvas::Frame,
};
use image::{DynamicImage, Rgba};
use imageproc::{drawing::draw_antialiased_line_segment_mut, pixelops::interpolate};

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
        let rgba = image.as_mut_rgba8().expect("image should be RGBA");
        let color = Rgba([
            (self.color.r * 255.0) as u8,
            (self.color.g * 255.0) as u8,
            (self.color.b * 255.0) as u8,
            (self.color.a * 255.0) as u8,
        ]);

        let segments = match self.kind {
            ShapeKind::Rectangle => {
                let r = normalize_rect(self.start, self.end);
                let tl = Point::new(r.x, r.y);
                let tr = Point::new(r.x + r.width, r.y);
                let br = Point::new(r.x + r.width, r.y + r.height);
                let bl = Point::new(r.x, r.y + r.height);
                vec![(tl, tr), (tr, br), (br, bl), (bl, tl)]
            }
            ShapeKind::Ellipse => {
                let r = normalize_rect(self.start, self.end);
                let cx = r.x + r.width / 2.0;
                let cy = r.y + r.height / 2.0;
                ellipse_segments(Point::new(cx, cy), r.width / 2.0, r.height / 2.0)
            }
            ShapeKind::Line => vec![(self.start, self.end)],
            ShapeKind::Arrow => arrow_segments(self.start, self.end),
            ShapeKind::Star => closed_segments(&star_vertices(self.start, self.end)),
            ShapeKind::Polygon => closed_segments(&polygon_vertices(self.start, self.end, 6)),
        };

        for (a, b) in segments {
            draw_antialiased_line_segment_mut(
                rgba,
                (a.x as i32, a.y as i32),
                (b.x as i32, b.y as i32),
                color,
                interpolate,
            );
        }

        *image = DynamicImage::ImageRgba8(rgba.clone());
    }

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        None // Already committed
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
