// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::Color;
use image::{DynamicImage, RgbaImage};
use tiny_skia::{LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

pub fn build_path(f: impl FnOnce(&mut PathBuilder)) -> Option<Path> {
    let mut path_builder = PathBuilder::new();
    f(&mut path_builder);
    path_builder.finish()
}

// Quantize a 0.0..=1.0 color component to an 8-bit channel. Rounds to nearest
// and clamps so out-of-gamut inputs cannot wrap. `as u8` on a float already
// saturates, but truncates instead of rounding, hence the explicit round.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // reason: value clamped to 0.0..=255.0 then rounded, in-range for u8
fn channel_u8(component: f32) -> u8 {
    (component * 255.0).round().clamp(0.0, 255.0) as u8
}

// Quantize an already-scaled 0.0..=255.0 intensity to an 8-bit channel.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // reason: value clamped to 0.0..=255.0 then rounded, in-range for u8
const fn intensity_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

/// Stroke a path onto a `DynamicImage` with the given color, width, and line cap.
///
/// # Panics
///
/// Panics if `image` is not RGBA, or if its dimensions cannot back a pixmap
/// (zero-sized or larger than tiny-skia supports).
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
        channel_u8(color.r),
        channel_u8(color.g),
        channel_u8(color.b),
        channel_u8(color.a),
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

/// Fill a path onto a `DynamicImage` with the given color.
///
/// # Panics
///
/// Panics if `image` is not RGBA, or if its dimensions cannot back a pixmap
/// (zero-sized or larger than tiny-skia supports).
pub fn fill_on_image(image: &mut DynamicImage, path: &Path, color: Color) {
    let rgba = image.as_mut_rgba8().expect("image should be RGBA");
    let (img_width, img_height) = (rgba.width(), rgba.height());

    let mut overlay =
        Pixmap::new(img_width, img_height).expect("image dimensions should produce a valid pixmap");
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        channel_u8(color.r),
        channel_u8(color.g),
        channel_u8(color.b),
        channel_u8(color.a),
    );

    overlay.fill_path(
        path,
        &paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    blend_overlay(rgba, &overlay);
}

fn blend_overlay(dst: &mut RgbaImage, overlay: &Pixmap) {
    let overlay_data = overlay.data();
    let dst_data: &mut [u8] = dst.as_mut();

    for (dst_pixel, src_chunk) in dst_data
        .chunks_exact_mut(4)
        .zip(overlay_data.chunks_exact(4))
    {
        let src_alpha = f32::from(src_chunk[3]) / 255.0;
        if src_alpha == 0.0 {
            continue;
        }

        // Un-premultiply the overlay
        let src_red = f32::from(src_chunk[0]) / src_alpha;
        let src_green = f32::from(src_chunk[1]) / src_alpha;
        let src_blue = f32::from(src_chunk[2]) / src_alpha;

        let dst_alpha = f32::from(dst_pixel[3]) / 255.0;
        let dst_red = f32::from(dst_pixel[0]);
        let dst_green = f32::from(dst_pixel[1]);
        let dst_blue = f32::from(dst_pixel[2]);

        // Source-over compositing
        let out_alpha = src_alpha + dst_alpha * (1.0 - src_alpha);

        if out_alpha > 0.0 {
            dst_pixel[0] = intensity_u8(
                (dst_red * dst_alpha).mul_add(1.0 - src_alpha, src_red * src_alpha) / out_alpha,
            );
            dst_pixel[1] = intensity_u8(
                (dst_green * dst_alpha).mul_add(1.0 - src_alpha, src_green * src_alpha) / out_alpha,
            );
            dst_pixel[2] = intensity_u8(
                (dst_blue * dst_alpha).mul_add(1.0 - src_alpha, src_blue * src_alpha) / out_alpha,
            );
            dst_pixel[3] = intensity_u8(out_alpha * 255.0);
        }
    }
}
