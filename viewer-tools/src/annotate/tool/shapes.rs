mod operation;
mod preview;

pub use operation::ShapeOperation;
pub use preview::ShapePreview;

use cosmic::{
    Renderer,
    iced::{Color, Point, Rectangle, Size},
    iced::widget::canvas::{Fill, Frame, LineCap, Path, Stroke, path::Builder},
    widget::canvas::LineJoin,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Line,
    Arrow,
    Star,
    Polygon,
}

/// Draw a shape overlay on the canvas.
pub fn draw_shape(
    kind: ShapeKind,
    start: Point,
    end: Point,
    color: Color,
    width: f32,
    frame: &mut Frame<Renderer>,
    _scale: f32,
) {
    let path = build_path(kind, start, end);

    match kind {
        ShapeKind::Star | ShapeKind::Polygon => {
            frame.fill(&path, Fill::from(color));
        }
        ShapeKind::Arrow => {
            // Stroke the shaft, fill the arrowhead
            let shaft = Path::line(start, end);
            let stroke = Stroke::default()
                .with_color(color)
                .with_width(width)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round);
            frame.stroke(&shaft, stroke);
            let head = arrow_head_path(start, end);
            frame.fill(&head, Fill::from(color));
        }
        _ => {
            let stroke = Stroke::default()
                .with_color(color)
                .with_width(width)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round);
            frame.stroke(&path, stroke);
        }
    }
}

fn build_path(kind: ShapeKind, start: Point, end: Point) -> Path {
    match kind {
        ShapeKind::Rectangle => {
            let rectangle = normalize_rect(start, end);
            Path::rectangle(rectangle.position(), rectangle.size())
        }
        ShapeKind::Ellipse => {
            let radius = normalize_rect(start, end);
            let center_x = radius.x + radius.width / 2.0;
            let center_y = radius.y + radius.height / 2.0;
            ellipse_path(
                Point::new(center_x, center_y),
                radius.width / 2.0,
                radius.height / 2.0,
            )
        }
        ShapeKind::Line => Path::line(start, end),
        ShapeKind::Arrow => arrow_path(start, end),
        ShapeKind::Star => closed_path(&star_vertices(start, end)),
        ShapeKind::Polygon => closed_path(&polygon_vertices(start, end, 6)),
    }
}

pub fn normalize_rect(a: Point, b: Point) -> Rectangle {
    Rectangle::new(
        Point::new(a.x.min(b.x), a.y.min(b.y)),
        Size::new((a.x - b.x).abs(), (a.y - b.y).abs()),
    )
}

#[allow(clippy::cast_precision_loss)] // reason: SEGMENTS and seg are small (<=64), exact in f32
fn ellipse_path(center: Point, radius_x: f32, radius_y: f32) -> Path {
    const SEGMENTS: usize = 64;
    Path::new(|builder: &mut Builder| {
        for seg in 0..SEGMENTS {
            let angle = 2.0 * std::f32::consts::PI * seg as f32 / SEGMENTS as f32;
            let point = Point::new(
                radius_x.mul_add(angle.cos(), center.x),
                radius_y.mul_add(angle.sin(), center.y),
            );
            if seg == 0 {
                builder.move_to(point);
            } else {
                builder.line_to(point);
            }
        }
        builder.close();
    })
}

fn arrow_path(start: Point, end: Point) -> Path {
    let delta_x = end.x - start.x;
    let delta_y = end.y - start.y;
    let len = delta_x.hypot(delta_y);
    if len < 1.0 {
        return Path::line(start, end);
    }

    let head_len = (len * 0.25).min(30.0);
    let head_width = head_len * 0.5;

    // Unit vectors: along line and perpendicular
    let unit_x = delta_x / len;
    let unit_y = delta_y / len;
    let point_x = -unit_y;
    let point_y = unit_x;

    let base = Point::new(unit_x.mul_add(-head_len, end.x), unit_y.mul_add(-head_len, end.y));
    let left = Point::new(
        base.x + point_x * head_width / 2.0,
        base.y + point_y * head_width / 2.0,
    );
    let right = Point::new(
        base.x - point_x * head_width / 2.0,
        base.y - point_y * head_width / 2.0,
    );

    Path::new(|builder: &mut Builder| {
        // Shaft
        builder.move_to(start);
        builder.line_to(end);
        // Arrowhead
        builder.move_to(left);
        builder.line_to(end);
        builder.line_to(right);
    })
}

/// Filled arrowhead triangle for the canvas preview.
fn arrow_head_path(start: Point, end: Point) -> Path {
    let delta_x = end.x - start.x;
    let delta_y = end.y - start.y;
    let len = delta_x.hypot(delta_y);
    if len < 1.0 {
        return Path::line(start, end);
    }

    let head_len = (len * 0.25).min(30.0);
    let head_width = head_len * 0.5;
    let unit_x = delta_x / len;
    let unit_y = delta_y / len;
    let perp_x = -unit_y;
    let perp_y = unit_x;

    let base = Point::new(unit_x.mul_add(-head_len, end.x), unit_y.mul_add(-head_len, end.y));
    let left = Point::new(
        base.x + perp_x * head_width / 2.0,
        base.y + perp_y * head_width / 2.0,
    );
    let right = Point::new(
        base.x - perp_x * head_width / 2.0,
        base.y - perp_y * head_width / 2.0,
    );

    Path::new(|builder: &mut Builder| {
        builder.move_to(end);
        builder.line_to(left);
        builder.line_to(right);
        builder.close();
    })
}

#[allow(clippy::cast_precision_loss)] // reason: point index/count are small (<=10), exact in f32
pub fn star_vertices(start: Point, end: Point) -> Vec<Point> {
    let bounds = normalize_rect(start, end);
    let center_x = bounds.x + bounds.width / 2.0;
    let center_y = bounds.y + bounds.height / 2.0;
    let outer_bounds_x = bounds.width / 2.0;
    let outer_bounds_y = bounds.height / 2.0;

    // Inner radius ~ 38% of outer for classic 5-pointed star
    let inner_bounds_x = outer_bounds_x * 0.38;
    let inner_bounds_y = outer_bounds_y * 0.38;

    let points = 5;
    let start_angle = -std::f32::consts::FRAC_PI_2;

    (0..points * 2)
        .map(|point| {
            let angle = start_angle + std::f32::consts::PI * point as f32 / points as f32;
            let (bounds_x, bounds_y) = if point % 2 == 0 {
                (outer_bounds_x, outer_bounds_y)
            } else {
                (inner_bounds_x, inner_bounds_y)
            };
            Point::new(
                bounds_x.mul_add(angle.cos(), center_x),
                bounds_y.mul_add(angle.sin(), center_y),
            )
        })
        .collect()
}

#[allow(clippy::cast_precision_loss)] // reason: side index/count are small (typically <=12), exact in f32
pub fn polygon_vertices(start: Point, end: Point, sides: usize) -> Vec<Point> {
    let bounds = normalize_rect(start, end);
    let center_x = bounds.x + bounds.width / 2.0;
    let center_y = bounds.y + bounds.height / 2.0;
    let bounds_x = bounds.width / 2.0;
    let bounds_y = bounds.height / 2.0;
    let start_angle = -std::f32::consts::FRAC_PI_2;

    (0..sides)
        .map(|side| {
            let angle = start_angle + 2.0 * std::f32::consts::PI * side as f32 / sides as f32;
            Point::new(
                center_x + bounds_x * angle.cos(),
                center_y + bounds_y * angle.sin(),
            )
        })
        .collect()
}

/// Build a closed Path from vertices.
fn closed_path(vertices: &[Point]) -> Path {
    Path::new(|builder: &mut Builder| {
        if let Some(first) = vertices.first() {
            builder.move_to(*first);
            for point in &vertices[1..] {
                builder.line_to(*point);
            }
            builder.close();
        }
    })
}

/// Build line segments for an arrow (shaft + head). Used by `apply()`.
pub fn arrow_segments(start: Point, end: Point) -> Vec<(Point, Point)> {
    let delta_x = end.x - start.x;
    let delta_y = end.y - start.y;
    let len = delta_x.hypot(delta_y);
    if len < 1.0 {
        return vec![(start, end)];
    }

    let head_len = (len * 0.25).min(30.0);
    let head_width = head_len * 0.5;
    let unit_x = delta_x / len;
    let unit_y = delta_y / len;
    let point_x = -unit_y;
    let point_y = unit_x;

    let base = Point::new(unit_x.mul_add(-head_len, end.x), unit_y.mul_add(-head_len, end.y));
    let left = Point::new(
        base.x + point_x * head_width / 2.0,
        base.y + point_y * head_width / 2.0,
    );
    let right = Point::new(
        base.x - point_x * head_width / 2.0,
        base.y - point_y * head_width / 2.0,
    );

    vec![
        // shaft
        (start, end),
        // left part of head
        (left, end),
        // right part of head
        (right, end),
    ]
}
