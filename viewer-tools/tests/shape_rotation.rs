// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::{Color, Point, Size};
use viewer_tools::ToolOperation;
use viewer_tools::annotate::{ShapeKind, ShapeOperation};
use viewer_tools::rotate::RotateDirection;

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

#[test]
fn shape_bounds_no_rotation() {
    let op = ShapeOperation::new(
        ShapeKind::Rectangle,
        Point::new(10.0, 20.0),
        Point::new(60.0, 80.0),
        RED,
        2.0,
    );
    let b = op.bounds();
    assert!((b.x - 10.0).abs() < 0.01);
    assert!((b.y - 20.0).abs() < 0.01);
    assert!((b.width - 50.0).abs() < 0.01);
    assert!((b.height - 60.0).abs() < 0.01);
}

#[test]
fn shape_bounds_reversed_points() {
    let op = ShapeOperation::new(
        ShapeKind::Rectangle,
        Point::new(60.0, 80.0),
        Point::new(10.0, 20.0),
        RED,
        2.0,
    );
    let b = op.bounds();
    assert!((b.x - 10.0).abs() < 0.01);
    assert!((b.y - 20.0).abs() < 0.01);
    assert!((b.width - 50.0).abs() < 0.01);
    assert!((b.height - 60.0).abs() < 0.01);
}

#[test]
fn shape_transform_rotate_right() {
    let mut op = ShapeOperation::new(
        ShapeKind::Rectangle,
        Point::new(10.0, 20.0),
        Point::new(30.0, 40.0),
        RED,
        2.0,
    );
    let image_size = Size::new(100.0, 200.0);

    op.transform_rotate(RotateDirection::Right, image_size);

    assert!((op.start.x - 180.0).abs() < 0.01);
    assert!((op.start.y - 10.0).abs() < 0.01);
    assert!((op.end.x - 160.0).abs() < 0.01);
    assert!((op.end.y - 30.0).abs() < 0.01);
}

#[test]
fn shape_transform_rotate_left() {
    let mut op = ShapeOperation::new(
        ShapeKind::Rectangle,
        Point::new(10.0, 20.0),
        Point::new(30.0, 40.0),
        RED,
        2.0,
    );
    let image_size = Size::new(100.0, 200.0);

    op.transform_rotate(RotateDirection::Left, image_size);

    assert!((op.start.x - 20.0).abs() < 0.01);
    assert!((op.start.y - 90.0).abs() < 0.01);
    assert!((op.end.x - 40.0).abs() < 0.01);
    assert!((op.end.y - 70.0).abs() < 0.01);
}

#[test]
fn shape_rotate_right_four_times_roundtrip() {
    let original_start = Point::new(10.0, 20.0);
    let original_end = Point::new(30.0, 40.0);
    let mut op = ShapeOperation::new(ShapeKind::Line, original_start, original_end, RED, 2.0);

    let size = Size::new(100.0, 100.0);
    for _ in 0..4 {
        op.transform_rotate(RotateDirection::Right, size);
    }

    assert!(
        (op.start.x - original_start.x).abs() < 0.01,
        "start.x should round-trip: {} vs {}",
        op.start.x,
        original_start.x,
    );
    assert!(
        (op.start.y - original_start.y).abs() < 0.01,
        "start.y should round-trip",
    );
    assert!((op.end.x - original_end.x).abs() < 0.01);
    assert!((op.end.y - original_end.y).abs() < 0.01);
}

#[test]
fn shape_hit_test_inside() {
    let op = ShapeOperation::new(
        ShapeKind::Rectangle,
        Point::new(10.0, 10.0),
        Point::new(90.0, 90.0),
        RED,
        4.0,
    );
    assert!(op.hit_test(Point::new(50.0, 50.0)));
}

#[test]
fn shape_hit_test_outside() {
    let op = ShapeOperation::new(
        ShapeKind::Rectangle,
        Point::new(10.0, 10.0),
        Point::new(90.0, 90.0),
        RED,
        4.0,
    );
    assert!(!op.hit_test(Point::new(200.0, 200.0)));
}

#[test]
fn shape_translate() {
    let mut op = ShapeOperation::new(
        ShapeKind::Rectangle,
        Point::new(10.0, 10.0),
        Point::new(50.0, 50.0),
        RED,
        2.0,
    );

    op.translate(20.0, 30.0);

    assert!((op.start.x - 30.0).abs() < 0.01);
    assert!((op.start.y - 40.0).abs() < 0.01);
    assert!((op.end.x - 70.0).abs() < 0.01);
    assert!((op.end.y - 80.0).abs() < 0.01);
}

#[test]
fn shape_movable() {
    let op = ShapeOperation::new(
        ShapeKind::Rectangle,
        Point::new(0.0, 0.0),
        Point::new(50.0, 50.0),
        RED,
        2.0,
    );
    assert!(ToolOperation::movable(&op));
}
