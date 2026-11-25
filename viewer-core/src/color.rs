// SPDX-License-Identifier: GPL-3.0-only

use image::{DynamicImage, Rgb, RgbImage};
use moxcms::{ColorProfile, DataColorSpace, Layout};

pub(crate) fn to_srgb(image: &mut DynamicImage, icc: Option<&[u8]>) {
    let Some(profile) = icc
        .filter(|p| p.len() <= 16 * 1024 * 1024)
        .and_then(|p| ColorProfile::new_from_slice(p).ok())
    else {
        return;
    };
    let srgb = ColorProfile::new_srgb();

    let use_gray = profile.color_space == DataColorSpace::Gray;
    let layout = if use_gray { Layout::Gray } else { Layout::Rgb };
    if !use_gray && profile.color_space != DataColorSpace::Rgb {
        return;
    }
    if image.color().bits_per_pixel() / u16::from(image.color().channel_count()) > 8 {
        let Ok(transform) =
            profile.create_transform_16bit(layout, &srgb, Layout::Rgb, Default::default())
        else {
            return;
        };
        let mut rgba = image.to_rgba16();
        let mut src = Vec::with_capacity(rgba.width() as usize * 3);
        let mut dst = vec![0; rgba.width() as usize * 3];
        let row_len = rgba.width() as usize * 4;
        for row in rgba.as_flat_samples_mut().samples.chunks_exact_mut(row_len) {
            src.clear();
            for pixel in row.chunks_exact(4) {
                src.extend_from_slice(&pixel[..if use_gray { 1 } else { 3 }]);
            }
            if transform.transform(&src, &mut dst).is_err() {
                return;
            }
            for (pixel, rgb) in row.chunks_exact_mut(4).zip(dst.chunks_exact(3)) {
                pixel[..3].copy_from_slice(rgb);
            }
        }
        *image = DynamicImage::ImageRgba16(rgba);
    } else {
        let Ok(transform) =
            profile.create_transform_8bit(layout, &srgb, Layout::Rgb, Default::default())
        else {
            return;
        };
        let mut rgba = image.to_rgba8();
        let row_len = rgba.width() as usize * 4;
        let mut src = Vec::with_capacity(rgba.width() as usize * 3);
        let mut dst = vec![0; rgba.width() as usize * 3];
        for row in rgba.as_flat_samples_mut().samples.chunks_exact_mut(row_len) {
            src.clear();
            for pixel in row.chunks_exact(4) {
                src.extend_from_slice(&pixel[..if use_gray { 1 } else { 3 }]);
            }
            if transform.transform(&src, &mut dst).is_err() {
                return;
            }
            for (pixel, rgb) in row.chunks_exact_mut(4).zip(dst.chunks_exact(3)) {
                pixel[..3].copy_from_slice(rgb);
            }
        }
        *image = DynamicImage::ImageRgba8(rgba);
    }
}

pub(crate) fn srgb_encode(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

pub(crate) fn map_linear_rgb([r, g, b]: [f32; 3]) -> [f32; 3] {
    let rgb = [r, g, b].map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 });
    let luminance = rgb[0].mul_add(0.2126, rgb[1].mul_add(0.7152, rgb[2] * 0.0722));
    rgb.map(|v| srgb_encode((v / (1.0 + luminance)).clamp(0.0, 1.0)))
}

pub(crate) fn display_rgba(image: &DynamicImage) -> image::RgbaImage {
    match image {
        DynamicImage::ImageRgb32F(buffer) => {
            let rgb = RgbImage::from_fn(buffer.width(), buffer.height(), |x, y| {
                Rgb(map_linear_rgb(buffer.get_pixel(x, y).0).map(|v| (v * 255.0).round() as u8))
            });
            DynamicImage::ImageRgb8(rgb).into_rgba8()
        }
        DynamicImage::ImageRgba32F(buffer) => {
            image::RgbaImage::from_fn(buffer.width(), buffer.height(), |x, y| {
                let [r, g, b, a] = buffer.get_pixel(x, y).0;
                let [r, g, b] = map_linear_rgb([r, g, b]);
                image::Rgba([r, g, b, a.clamp(0.0, 1.0)].map(|v| (255.0 * v).round() as u8))
            })
        }
        _ => image.to_rgba8(),
    }
}

pub(crate) fn convert_cicp(
    image: &mut DynamicImage,
    icc: Option<&[u8]>,
    primaries: u32,
    transfer: u32,
) {
    let hdr = matches!(transfer, 16 | 18);
    if !hdr && icc.is_some() {
        to_srgb(image, icc);
        return;
    }
    if !hdr && matches!(primaries, 0..=2) && matches!(transfer, 0..=2 | 13) {
        return;
    }
    let DynamicImage::ImageRgba16(buffer) = image else {
        return;
    };
    let inverse_transfer = |v: f32| match transfer {
        16 => {
            let p = v.powf(32.0 / 2523.0);
            ((p - 3424.0 / 4096.0).max(0.0) / (2413.0 / 128.0 - 2392.0 / 128.0 * p))
                .powf(16384.0 / 2610.0)
                * (10000.0 / 203.0)
        }
        18 => {
            if v <= 0.5 {
                v * v / 3.0
            } else {
                (((v - 0.559_910_7) / 0.178_832_77).exp() + 0.284_668_92) / 12.0
            }
        }
        1 | 6 | 14 | 15 => {
            if v < 0.081 {
                v / 4.5
            } else {
                ((v + 0.099) / 1.099).powf(1.0 / 0.45)
            }
        }
        4 => v.powf(2.2),
        5 => v.powf(2.8),
        8 => v,
        _ => {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
    };
    for pixel in buffer.pixels_mut() {
        let [mut r, mut g, mut b, _] = pixel.0.map(|v| f32::from(v) / 65535.0);
        [r, g, b] = [r, g, b].map(inverse_transfer);
        if transfer == 18 {
            let scene_y = (0.2627 * r + 0.6780 * g + 0.0593 * b).max(0.0);
            let ootf = scene_y.powf(0.2) * (1000.0 / 203.0);
            [r, g, b] = [r * ootf, g * ootf, b * ootf];
        }
        let rgb = match primaries {
            9 => [
                1.6605 * r - 0.5876 * g - 0.0728 * b,
                -0.1246 * r + 1.1329 * g - 0.0083 * b,
                -0.0182 * r - 0.1006 * g + 1.1187 * b,
            ],
            12 => [
                1.2249 * r - 0.2249 * g,
                -0.0421 * r + 1.0421 * g,
                -0.0196 * r - 0.0786 * g + 1.0983 * b,
            ],
            _ => [r, g, b],
        };
        let rgb = if hdr {
            map_linear_rgb(rgb)
        } else {
            rgb.map(|v| srgb_encode(v.clamp(0.0, 1.0)))
        };
        for (sample, v) in pixel.0.iter_mut().zip(rgb) {
            *sample = (v * 65535.0).round() as u16;
        }
    }
}
