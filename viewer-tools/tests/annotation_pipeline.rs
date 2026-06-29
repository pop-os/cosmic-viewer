use cosmic::iced::{Color, Point, Rectangle, Size};
use image::DynamicImage;
use viewer_tools::ToolOperation;
use viewer_tools::annotate::{
    HighlighterOperation, PenOperation, PencilOperation, ShapeKind, ShapeOperation,
};
use viewer_tools::rotate::{RotateDirection, RotateOperation};

fn white_image(w: u32, h: u32) -> DynamicImage {
    DynamicImage::new_rgba8(w, h)
}

fn pixel_sum(img: &DynamicImage) -> u64 {
    img.as_rgba8()
        .unwrap()
        .pixels()
        .map(|p| p.0.iter().map(|&b| u64::from(b)).sum::<u64>())
        .sum()
}

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

#[test]
fn shape_ellipse_modifies_image() {
    let mut img = white_image(100, 100);
    let before = pixel_sum(&img);

    let op = ShapeOperation::new(
        ShapeKind::Ellipse,
        Point::new(10.0, 10.0),
        Point::new(90.0, 90.0),
        RED,
        3.0,
    );
    op.apply(&mut img);

    assert_ne!(pixel_sum(&img), before);
}

#[test]
fn shape_line_modifies_image() {
    let mut img = white_image(100, 100);
    let before = pixel_sum(&img);

    let op = ShapeOperation::new(
        ShapeKind::Line,
        Point::new(0.0, 0.0),
        Point::new(99.0, 99.0),
        RED,
        5.0,
    );
    op.apply(&mut img);

    assert_ne!(pixel_sum(&img), before);
}

#[test]
fn pen_operation_modifies_image() {
    let mut img = white_image(100, 100);
    let before = pixel_sum(&img);

    let op = PenOperation {
        points: vec![
            Point::new(10.0, 10.0),
            Point::new(50.0, 50.0),
            Point::new(90.0, 10.0),
        ],
        color: RED,
        width: 4.0,
    };
    op.apply(&mut img);

    assert_ne!(pixel_sum(&img), before);
}

#[test]
fn pencil_operation_modifies_image() {
    let mut img = white_image(100, 100);
    let before = pixel_sum(&img);

    let op = PencilOperation {
        points: vec![Point::new(5.0, 5.0), Point::new(95.0, 95.0)],
        color: RED,
        width: 3.0,
    };
    op.apply(&mut img);

    assert_ne!(pixel_sum(&img), before);
}

#[test]
fn highlighter_operation_modifies_image() {
    let mut img = white_image(100, 100);
    let before = pixel_sum(&img);

    let op = HighlighterOperation {
        points: vec![Point::new(10.0, 50.0), Point::new(90.0, 50.0)],
        color: Color {
            r: 1.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        },
        width: 20.0,
    };
    op.apply(&mut img);

    assert_ne!(pixel_sum(&img), before);
}

#[test]
fn rotate_transforms_dimensions() {
    let mut img = DynamicImage::new_rgba8(200, 100);
    assert_eq!(img.width(), 200);
    assert_eq!(img.height(), 100);

    let op = RotateOperation::new(RotateDirection::Right);
    op.apply(&mut img);

    assert_eq!(img.width(), 100);
    assert_eq!(img.height(), 200);
}

#[test]
fn rotate_four_times_restores_dimensions() {
    let mut img = DynamicImage::new_rgba8(200, 100);
    let op = RotateOperation::new(RotateDirection::Right);

    for _ in 0..4 {
        op.apply(&mut img);
    }

    assert_eq!(img.width(), 200);
    assert_eq!(img.height(), 100);
}

#[test]
fn rotate_left_then_right_restores() {
    let mut img = DynamicImage::new_rgba8(200, 100);

    let left = RotateOperation::new(RotateDirection::Left);
    left.apply(&mut img);
    assert_eq!(img.width(), 100);
    assert_eq!(img.height(), 200);

    let right = RotateOperation::new(RotateDirection::Right);
    right.apply(&mut img);
    assert_eq!(img.width(), 200);
    assert_eq!(img.height(), 100);
}

#[test]
fn pen_transform_rotate_right() {
    let mut op = PenOperation {
        points: vec![Point::new(10.0, 20.0)],
        color: RED,
        width: 2.0,
    };
    let size = Size::new(100.0, 200.0);

    op.transform_rotate(RotateDirection::Right, size);

    let p = op.points[0];
    assert!(
        (p.x - 180.0).abs() < 0.01,
        "x should be height - y = 200 - 20 = 180"
    );
    assert!((p.y - 10.0).abs() < 0.01, "y should be original x = 10");
}

#[test]
fn pen_transform_crop() {
    let mut op = PenOperation {
        points: vec![Point::new(50.0, 60.0)],
        color: RED,
        width: 2.0,
    };

    let region = Rectangle::new(Point::new(10.0, 20.0), Size::new(80.0, 80.0));
    op.transform_crop(region);

    let p = op.points[0];
    assert!((p.x - 40.0).abs() < 0.01);
    assert!((p.y - 40.0).abs() < 0.01);
}

#[test]
fn shape_commit_returns_none() {
    let op = ShapeOperation::new(
        ShapeKind::Rectangle,
        Point::new(0.0, 0.0),
        Point::new(50.0, 50.0),
        RED,
        2.0,
    );
    assert!(
        op.commit().is_none(),
        "committed operations return None from commit()"
    );
}

#[test]
fn pen_commit_returns_none() {
    let op = PenOperation {
        points: vec![Point::new(0.0, 0.0), Point::new(10.0, 10.0)],
        color: RED,
        width: 2.0,
    };
    assert!(op.commit().is_none());
}

#[test]
fn rotate_commit_returns_none() {
    let op = RotateOperation::new(RotateDirection::Left);
    assert!(op.commit().is_none());
}

#[test]
fn pen_empty_points_no_op() {
    let mut img = white_image(50, 50);
    let before = pixel_sum(&img);

    let op = PenOperation {
        points: vec![],
        color: RED,
        width: 5.0,
    };
    op.apply(&mut img);

    assert_eq!(pixel_sum(&img), before, "empty pen should be a no-op");
}

#[test]
fn pen_single_point_no_op() {
    let mut img = white_image(50, 50);
    let before = pixel_sum(&img);

    let op = PenOperation {
        points: vec![Point::new(25.0, 25.0)],
        color: RED,
        width: 5.0,
    };
    op.apply(&mut img);

    assert_eq!(
        pixel_sum(&img),
        before,
        "single point pen should be a no-op"
    );
}

#[test]
fn shape_arrow_modifies_image() {
    let mut img = white_image(200, 200);
    let before = pixel_sum(&img);

    let op = ShapeOperation::new(
        ShapeKind::Arrow,
        Point::new(20.0, 100.0),
        Point::new(180.0, 100.0),
        RED,
        3.0,
    );
    op.apply(&mut img);

    assert_ne!(pixel_sum(&img), before);
}

#[test]
fn shape_star_modifies_image() {
    let mut img = white_image(200, 200);
    let before = pixel_sum(&img);

    let op = ShapeOperation::new(
        ShapeKind::Star,
        Point::new(10.0, 10.0),
        Point::new(190.0, 190.0),
        RED,
        2.0,
    );
    op.apply(&mut img);

    assert_ne!(pixel_sum(&img), before);
}
