use cosmic::iced::Color;
use image::{DynamicImage, RgbaImage};
use tiny_skia::{LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

pub fn build_path(f: impl FnOnce(&mut PathBuilder)) -> Option<Path> {
    let mut path_builder = PathBuilder::new();
    f(&mut path_builder);
    path_builder.finish()
}

/// Stroke a path onto a `DynamicImage` with the given color, width, and line cap.
pub fn stroke_on_image(
    image: &mut DynamicImage,
    path: &Path,
    color: Color,
    width: f32,
    line_cap: LineCap,
) {
    let rgba = image.as_mut_rgba8().expect("image should be RGBA");
    let (img_width, img_height) = (rgba.width(), rgba.height());

    // Render stroke onto a transparent overlay to avoid premultiplication issues
    let mut overlay =
        Pixmap::new(img_width, img_height).expect("image dimensions should produce a valid pixmap");
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        (color.r * 255.0) as u8,
        (color.g * 255.0) as u8,
        (color.b * 255.0) as u8,
        (color.a * 255.0) as u8,
    );

    let stroke = Stroke {
        width,
        line_cap,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };

    overlay.stroke_path(path, &paint, &stroke, Transform::identity(), None);

    // Composite overlay onto the image buffer
    blend_overlay(rgba, &overlay);
}

fn blend_overlay(dst: &mut RgbaImage, overlay: &Pixmap) {
    let overlay_data = overlay.data();
    let dst_data: &mut [u8] = dst.as_mut();

    for (dst_pixel, src_chunk) in dst_data
        .chunks_exact_mut(4)
        .zip(overlay_data.chunks_exact(4))
    {
        let src_alpha = src_chunk[3] as f32 / 255.0;
        if src_alpha == 0.0 {
            continue;
        }

        // Un-premultiply the overlay
        let src_red = src_chunk[0] as f32 / src_alpha;
        let src_green = src_chunk[1] as f32 / src_alpha;
        let src_blue = src_chunk[2] as f32 / src_alpha;

        let dst_alpha = dst_pixel[3] as f32 / 255.0;
        let dst_red = dst_pixel[0] as f32;
        let dst_green = dst_pixel[1] as f32;
        let dst_blue = dst_pixel[2] as f32;

        // Source-over compositing
        let out_alpha = src_alpha + dst_alpha * (1.0 - src_alpha);

        if out_alpha > 0.0 {
            dst_pixel[0] = ((src_red * src_alpha + dst_red * dst_alpha * (1.0 - src_alpha))
                / out_alpha)
                .min(255.0) as u8;
            dst_pixel[1] = ((src_green * src_alpha + dst_green * dst_alpha * (1.0 - src_alpha))
                / out_alpha)
                .min(255.0) as u8;
            dst_pixel[2] = ((src_blue * src_alpha + dst_blue * dst_alpha * (1.0 - src_alpha))
                / out_alpha)
                .min(255.0) as u8;
            dst_pixel[3] = (out_alpha * 255.0).min(255.0) as u8;
        }
    }
}
