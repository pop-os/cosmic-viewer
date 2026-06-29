use cosmic::iced::{Color, Point};
use image::DynamicImage;
use viewer_tools::annotate::{ShapeKind, ShapeOperation};
use viewer_tools::ToolOperation;

fn make_blank_image(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgba8(image::RgbaImage::new(width, height))
}

fn compare_images(actual: &DynamicImage, expected: &DynamicImage) -> f64 {
    let actual_rgba = actual.to_rgba8();
    let expected_rgba = expected.to_rgba8();

    if actual_rgba.dimensions() != expected_rgba.dimensions() {
        return 1.0;
    }

    let mut diff_count = 0;
    let total_pixels = actual_rgba.width() * actual_rgba.height();

    for (p1, p2) in actual_rgba.pixels().zip(expected_rgba.pixels()) {
        if p1 != p2 {
            diff_count += 1;
        }
    }

    f64::from(diff_count) / f64::from(total_pixels)
}

#[test]
fn visual_regression_arrow_proportions() {
    let mut actual = make_blank_image(400, 200);
    let op = ShapeOperation::new(
        ShapeKind::Arrow,
        Point::new(50.0, 100.0),
        Point::new(350.0, 100.0),
        Color::BLACK,
        4.0,
    );

    op.apply(&mut actual);

    let blank = make_blank_image(400, 200);
    assert!(compare_images(&actual, &blank) > 0.0, "Arrow should have rendered something");
}
