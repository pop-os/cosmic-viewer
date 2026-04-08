use cosmic::iced::{Color, Point};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use image::DynamicImage;
use viewer_tools::{
    ToolOperation,
    annotate::{PolygonPreset, ShapeKind, ShapeOperation},
    renderer::{build_path, fill_on_image, stroke_on_image},
};

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

fn make_test_image() -> DynamicImage {
    DynamicImage::ImageRgba8(image::RgbaImage::new(1000, 1000))
}

fn rect_op(fill: bool) -> ShapeOperation {
    let mut op = ShapeOperation::new(
        ShapeKind::Rectangle,
        Point::new(100.0, 100.0),
        Point::new(900.0, 900.0),
        RED,
        4.0,
    );
    if fill {
        op.fill_color = Some(RED);
        op.stroke_color = None;
    }
    op
}

fn ellipse_op(fill: bool) -> ShapeOperation {
    let mut op = ShapeOperation::new(
        ShapeKind::Ellipse,
        Point::new(100.0, 100.0),
        Point::new(900.0, 900.0),
        RED,
        4.0,
    );
    if fill {
        op.fill_color = Some(RED);
        op.stroke_color = None;
    }
    op
}

fn polygon_op(preset: PolygonPreset) -> ShapeOperation {
    let mut op = ShapeOperation::new(
        ShapeKind::Polygon,
        Point::new(100.0, 100.0),
        Point::new(900.0, 900.0),
        RED,
        4.0,
    );
    op.polygon_preset = preset;
    op.fill_color = Some(RED);
    op.stroke_color = None;
    op
}

fn arrow_op() -> ShapeOperation {
    let mut op = ShapeOperation::new(
        ShapeKind::Arrow,
        Point::new(100.0, 500.0),
        Point::new(900.0, 500.0),
        RED,
        4.0,
    );
    op.fill_color = Some(RED);
    op.stroke_color = None;
    op
}

fn star_op() -> ShapeOperation {
    let mut op = ShapeOperation::new(
        ShapeKind::Star,
        Point::new(100.0, 100.0),
        Point::new(900.0, 900.0),
        RED,
        4.0,
    );
    op.fill_color = Some(RED);
    op.stroke_color = None;
    op
}

// --- ShapeOperation::apply() benchmarks ---

fn bench_apply_rect_fill(c: &mut Criterion) {
    let op = rect_op(true);
    c.bench_function("shape_apply/rect_fill_1000x1000", |b| {
        b.iter(|| {
            let mut img = make_test_image();
            let rgba = img.as_mut_rgba8().expect("RGBA");
            let (w, h) = rgba.dimensions();
            let mut pixmap = tiny_skia::PixmapMut::from_bytes(rgba.as_mut(), w, h).expect("valid");
            op.apply(&mut pixmap);
            black_box(&img);
        });
    });
}

fn bench_apply_rect_stroke(c: &mut Criterion) {
    let mut op = rect_op(false);
    op.fill_color = None;
    op.stroke_color = Some(RED);
    c.bench_function("shape_apply/rect_stroke_1000x1000", |b| {
        b.iter(|| {
            let mut img = make_test_image();
            let rgba = img.as_mut_rgba8().expect("RGBA");
            let (w, h) = rgba.dimensions();
            let mut pixmap = tiny_skia::PixmapMut::from_bytes(rgba.as_mut(), w, h).expect("valid");
            op.apply(&mut pixmap);
            black_box(&img);
        });
    });
}

fn bench_apply_ellipse_fill(c: &mut Criterion) {
    let op = ellipse_op(true);
    c.bench_function("shape_apply/ellipse_fill_1000x1000", |b| {
        b.iter(|| {
            let mut img = make_test_image();
            let rgba = img.as_mut_rgba8().expect("RGBA");
            let (w, h) = rgba.dimensions();
            let mut pixmap = tiny_skia::PixmapMut::from_bytes(rgba.as_mut(), w, h).expect("valid");
            op.apply(&mut pixmap);
            black_box(&img);
        });
    });
}

fn bench_apply_triangle_fill(c: &mut Criterion) {
    let op = polygon_op(PolygonPreset::Triangle);
    c.bench_function("shape_apply/triangle_fill_1000x1000", |b| {
        b.iter(|| {
            let mut img = make_test_image();
            let rgba = img.as_mut_rgba8().expect("RGBA");
            let (w, h) = rgba.dimensions();
            let mut pixmap = tiny_skia::PixmapMut::from_bytes(rgba.as_mut(), w, h).expect("valid");
            op.apply(&mut pixmap);
            black_box(&img);
        });
    });
}

fn bench_apply_arrow(c: &mut Criterion) {
    let op = arrow_op();
    c.bench_function("shape_apply/arrow_1000x1000", |b| {
        b.iter(|| {
            let mut img = make_test_image();
            let rgba = img.as_mut_rgba8().expect("RGBA");
            let (w, h) = rgba.dimensions();
            let mut pixmap = tiny_skia::PixmapMut::from_bytes(rgba.as_mut(), w, h).expect("valid");
            op.apply(&mut pixmap);
            black_box(&img);
        });
    });
}

fn bench_apply_star(c: &mut Criterion) {
    let op = star_op();
    c.bench_function("shape_apply/star_1000x1000", |b| {
        b.iter(|| {
            let mut img = make_test_image();
            let rgba = img.as_mut_rgba8().expect("RGBA");
            let (w, h) = rgba.dimensions();
            let mut pixmap = tiny_skia::PixmapMut::from_bytes(rgba.as_mut(), w, h).expect("valid");
            op.apply(&mut pixmap);
            black_box(&img);
        });
    });
}

// --- Rotation vs no rotation ---

fn bench_apply_rect_with_rotation(c: &mut Criterion) {
    let mut op = rect_op(true);
    op.rotation = std::f32::consts::FRAC_PI_4;
    c.bench_function("shape_apply/rect_fill_rotated_1000x1000", |b| {
        b.iter(|| {
            let mut img = make_test_image();
            let rgba = img.as_mut_rgba8().expect("RGBA");
            let (w, h) = rgba.dimensions();
            let mut pixmap = tiny_skia::PixmapMut::from_bytes(rgba.as_mut(), w, h).expect("valid");
            op.apply(&mut pixmap);
            black_box(&img);
        });
    });
}

fn bench_apply_rect_without_rotation(c: &mut Criterion) {
    let op = rect_op(true);
    c.bench_function("shape_apply/rect_fill_no_rotation_1000x1000", |b| {
        b.iter(|| {
            let mut img = make_test_image();
            let rgba = img.as_mut_rgba8().expect("RGBA");
            let (w, h) = rgba.dimensions();
            let mut pixmap = tiny_skia::PixmapMut::from_bytes(rgba.as_mut(), w, h).expect("valid");
            op.apply(&mut pixmap);
            black_box(&img);
        });
    });
}

// --- fill_on_image vs stroke_on_image ---

fn bench_fill_vs_stroke(c: &mut Criterion) {
    let rect_path = build_path(|pb| {
        if let Some(rect) = tiny_skia::Rect::from_xywh(100.0, 100.0, 800.0, 800.0) {
            pb.push_rect(rect);
        }
    });
    let Some(path) = rect_path else { return };

    c.bench_function("renderer/fill_on_image_1000x1000", |b| {
        b.iter(|| {
            let mut img = make_test_image();
            fill_on_image(&mut img, &path, RED);
            black_box(&img);
        });
    });

    c.bench_function("renderer/stroke_on_image_1000x1000", |b| {
        b.iter(|| {
            let mut img = make_test_image();
            stroke_on_image(&mut img, &path, RED, 4.0, tiny_skia::LineCap::Round);
            black_box(&img);
        });
    });
}

// --- build_path for different shape types ---

fn bench_build_path_shapes(c: &mut Criterion) {
    c.bench_function("renderer/build_path_rect", |b| {
        b.iter(|| {
            let path = build_path(|pb| {
                if let Some(rect) = tiny_skia::Rect::from_xywh(100.0, 100.0, 800.0, 800.0) {
                    pb.push_rect(rect);
                }
            });
            black_box(path);
        });
    });

    c.bench_function("renderer/build_path_ellipse", |b| {
        b.iter(|| {
            let path = build_path(|pb| {
                let rect = tiny_skia::Rect::from_xywh(100.0, 100.0, 800.0, 800.0)
                    .unwrap_or(tiny_skia::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap());
                pb.push_oval(rect);
            });
            black_box(path);
        });
    });

    c.bench_function("renderer/build_path_line", |b| {
        b.iter(|| {
            let path = build_path(|pb| {
                pb.move_to(100.0, 100.0);
                pb.line_to(900.0, 900.0);
            });
            black_box(path);
        });
    });

    c.bench_function("renderer/build_path_polygon_6", |b| {
        b.iter(|| {
            let sides = 6;
            let path = build_path(|pb| {
                let cx = 500.0f32;
                let cy = 500.0f32;
                let rx = 400.0f32;
                let ry = 400.0f32;
                let start_angle = -std::f32::consts::FRAC_PI_2;
                for i in 0..=sides {
                    let angle = start_angle + 2.0 * std::f32::consts::PI * i as f32 / sides as f32;
                    let x = cx + rx * angle.cos();
                    let y = cy + ry * angle.sin();
                    if i == 0 {
                        pb.move_to(x, y);
                    } else {
                        pb.line_to(x, y);
                    }
                }
                pb.close();
            });
            black_box(path);
        });
    });

    c.bench_function("renderer/build_path_star", |b| {
        b.iter(|| {
            let path = build_path(|pb| {
                let cx = 500.0f32;
                let cy = 500.0f32;
                let outer_r = 400.0f32;
                let inner_r = outer_r * 0.38;
                let points = 5;
                let start_angle = -std::f32::consts::FRAC_PI_2;
                for i in 0..points * 2 {
                    let angle = start_angle + std::f32::consts::PI * i as f32 / points as f32;
                    let r = if i % 2 == 0 { outer_r } else { inner_r };
                    let x = cx + r * angle.cos();
                    let y = cy + r * angle.sin();
                    if i == 0 {
                        pb.move_to(x, y);
                    } else {
                        pb.line_to(x, y);
                    }
                }
                pb.close();
            });
            black_box(path);
        });
    });
}

// --- ShapeOperation::hit_test benchmarks ---

fn bench_hit_test(c: &mut Criterion) {
    let op = rect_op(true);

    c.bench_function("shape/hit_test_inside", |b| {
        b.iter(|| black_box(op.hit_test(Point::new(500.0, 500.0))));
    });

    c.bench_function("shape/hit_test_outside", |b| {
        b.iter(|| black_box(op.hit_test(Point::new(2000.0, 2000.0))));
    });

    c.bench_function("shape/hit_test_edge", |b| {
        b.iter(|| black_box(op.hit_test(Point::new(100.0, 100.0))));
    });
}

// --- ShapeOperation::bounds benchmarks ---

fn bench_bounds(c: &mut Criterion) {
    let op = rect_op(true);
    c.bench_function("shape/bounds_start_end", |b| {
        b.iter(|| black_box(op.bounds()));
    });

    let verts = (0..20)
        .map(|i| {
            let angle = 2.0 * std::f32::consts::PI * i as f32 / 20.0;
            Point::new(500.0 + 400.0 * angle.cos(), 500.0 + 400.0 * angle.sin())
        })
        .collect();
    let poly_op = ShapeOperation::new_polygon(verts, RED, 4.0);
    c.bench_function("shape/bounds_20_vertices", |b| {
        b.iter(|| black_box(poly_op.bounds()));
    });
}

// --- ShapeOperation::translate benchmarks ---

fn bench_translate(c: &mut Criterion) {
    c.bench_function("shape/translate_start_end", |b| {
        let mut op = rect_op(true);
        b.iter(|| {
            op.translate(1.0, 1.0);
            black_box(&op);
        });
    });

    let verts: Vec<Point> = (0..100)
        .map(|i| {
            let angle = 2.0 * std::f32::consts::PI * i as f32 / 100.0;
            Point::new(500.0 + 400.0 * angle.cos(), 500.0 + 400.0 * angle.sin())
        })
        .collect();
    c.bench_function("shape/translate_100_vertices", |b| {
        let mut op = ShapeOperation::new_polygon(verts.clone(), RED, 4.0);
        b.iter(|| {
            op.translate(1.0, 1.0);
            black_box(&op);
        });
    });
}

// --- Apply different polygon side counts ---

fn bench_apply_polygon_sides(c: &mut Criterion) {
    for sides in [3, 4, 5, 6, 8, 12, 20] {
        let preset = match sides {
            3 => PolygonPreset::Triangle,
            4 => PolygonPreset::Quad,
            5 => PolygonPreset::Pentagon,
            6 => PolygonPreset::Hexagon,
            8 => PolygonPreset::Octagon,
            _ => PolygonPreset::Hexagon, // Fallback; sides clamped in apply
        };
        let op = polygon_op(preset);
        c.bench_function(
            &format!("shape_apply/polygon_{sides}_sides_1000x1000"),
            |b| {
                b.iter(|| {
                    let mut img = make_test_image();
                    let rgba = img.as_mut_rgba8().expect("RGBA");
                    let (w, h) = rgba.dimensions();
                    let mut pixmap =
                        tiny_skia::PixmapMut::from_bytes(rgba.as_mut(), w, h).expect("valid");
                    op.apply(&mut pixmap);
                    black_box(&img);
                });
            },
        );
    }
}

fn make_test_image_4k() -> DynamicImage {
    DynamicImage::ImageRgba8(image::RgbaImage::new(3840, 2160))
}

// --- Complex Scenarios ---

fn bench_apply_100_overlapping_rects(c: &mut Criterion) {
    let mut ops = Vec::new();
    for i in 0..100 {
        let mut op = ShapeOperation::new(
            ShapeKind::Rectangle,
            Point::new(i as f32 * 5.0, i as f32 * 5.0),
            Point::new(i as f32 * 5.0 + 500.0, i as f32 * 5.0 + 500.0),
            RED,
            2.0,
        );
        op.fill_color = Some(Color::from_rgba(1.0, 0.0, 0.0, 0.5)); // Semi-transparent
        ops.push(op);
    }

    let boxed_ops: Vec<Box<dyn ToolOperation>> = ops
        .into_iter()
        .map(|o| Box::new(o) as Box<dyn ToolOperation>)
        .collect();

    c.bench_function("complex/100_rects_1000x1000", |b| {
        b.iter(|| {
            let mut img = make_test_image();
            viewer_tools::batch_apply(&boxed_ops, &mut img);
            black_box(&img);
        });
    });
}

fn bench_apply_100_complex_polygons_4k(c: &mut Criterion) {
    let mut ops = Vec::new();
    for i in 0..100 {
        let mut op = ShapeOperation::new(
            ShapeKind::Polygon,
            Point::new(i as f32 * 10.0, i as f32 * 10.0),
            Point::new(i as f32 * 10.0 + 800.0, i as f32 * 10.0 + 800.0),
            RED,
            4.0,
        );
        op.polygon_preset = PolygonPreset::Hexagon;
        op.fill_color = Some(RED);
        ops.push(op);
    }

    let boxed_ops: Vec<Box<dyn ToolOperation>> = ops
        .into_iter()
        .map(|o| Box::new(o) as Box<dyn ToolOperation>)
        .collect();

    c.bench_function("complex/100_polygons_4k", |b| {
        b.iter(|| {
            let mut img = make_test_image_4k();
            viewer_tools::batch_apply(&boxed_ops, &mut img);
            black_box(&img);
        });
    });
}

// --- High Resolution ---

fn bench_apply_rect_fill_4k(c: &mut Criterion) {
    let op = rect_op(true);
    c.bench_function("shape_apply/rect_fill_4k", |b| {
        b.iter(|| {
            let mut img = make_test_image_4k();
            let rgba = img.as_mut_rgba8().expect("RGBA");
            let (w, h) = rgba.dimensions();
            let mut pixmap = tiny_skia::PixmapMut::from_bytes(rgba.as_mut(), w, h).expect("valid");
            op.apply(&mut pixmap);
            black_box(&img);
        });
    });
}

criterion_group!(
    shape_apply_benches,
    bench_apply_rect_fill,
    bench_apply_rect_stroke,
    bench_apply_ellipse_fill,
    bench_apply_triangle_fill,
    bench_apply_arrow,
    bench_apply_star,
    bench_apply_polygon_sides,
    bench_apply_rect_fill_4k,
);

criterion_group!(
    complex_benches,
    bench_apply_100_overlapping_rects,
    bench_apply_100_complex_polygons_4k,
);

criterion_group!(
    rotation_benches,
    bench_apply_rect_with_rotation,
    bench_apply_rect_without_rotation,
);

criterion_group!(
    renderer_benches,
    bench_fill_vs_stroke,
    bench_build_path_shapes,
);

criterion_group!(
    shape_ops_benches,
    bench_hit_test,
    bench_bounds,
    bench_translate,
);

criterion_main!(
    shape_apply_benches,
    complex_benches,
    rotation_benches,
    renderer_benches,
    shape_ops_benches
);
