// SPDX-License-Identifier: GPL-3.0-only

use crate::{Format, animation, color, detect, native, pages};
use cosmic::widget::image::Handle;
use fast_image_resize::{PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image as FirImage};
use image::{
    DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbaImage, metadata::Orientation,
};
use libheif_rs::{ColorSpace as HeifColorSpace, HeifContext, LibHeif, RgbChroma, StreamReader};
use resvg::{tiny_skia, usvg};
use std::{
    fmt::{self, Debug, Formatter},
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};
use thiserror::Error;
use turbojpeg::{Decompressor, Image, PixelFormat, ScalingFactor};

// Cap texture uploads at 2048px on the long edge; the full-resolution image
// is kept separately so edits and saves operate on the real pixels, not the texture.
pub(crate) const MAX_TEX: u32 = 2048;

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("Failed to read file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to decode image: {0}")]
    Decode(#[from] image::ImageError),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("{0}")]
    Invalid(String),
    #[error("Image dimensions exceed the decode limit")]
    Limit,
    #[error("Task cancelled")]
    Cancelled,
}

#[derive(Clone)]
pub struct LoadedImage {
    pub handle: Handle,
    pub image: DynamicImage,
    pub width: u32,
    pub height: u32,
    pub path: PathBuf,
}

impl Debug for LoadedImage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // `handle` and `image` hold opaque pixel buffers; omit them from Debug.
        f.debug_struct("LoadedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

pub(crate) const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_SIDE: u32 = 32_768;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;

pub(crate) fn pixel_count(w: u32, h: u32) -> Result<usize, LoadError> {
    let count = u64::from(w) * u64::from(h);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || count > MAX_PIXELS {
        return Err(LoadError::Limit);
    }
    Ok(count as usize)
}

pub(crate) fn decode_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits
}

pub(crate) fn bounded_file(path: &Path) -> Result<File, LoadError> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(LoadError::Invalid(
            "Image exceeds the 512 MiB input limit or is not a regular file".into(),
        ));
    }
    Ok(file)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, LoadError> {
    let mut data = Vec::new();
    bounded_file(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > MAX_FILE_BYTES {
        return Err(LoadError::Invalid(
            "Image exceeds the 512 MiB input limit".into(),
        ));
    }
    Ok(data)
}

pub(crate) fn require_format(path: &Path) -> Result<Format, LoadError> {
    bounded_file(path)?;
    detect(path)?.ok_or_else(|| LoadError::UnsupportedFormat("Unknown image signature".into()))
}

pub fn display_handle(image: &DynamicImage) -> Handle {
    let rgba = color::display_rgba(image);
    let (width, height) = rgba.dimensions();
    if (width > MAX_TEX || height > MAX_TEX)
        && let Ok((tw, th, pixels)) = fast_resize_rgba(rgba.as_raw(), width, height, MAX_TEX)
    {
        return Handle::from_rgba(tw, th, pixels);
    }
    Handle::from_rgba(width, height, rgba.into_raw())
}

impl LoadedImage {
    pub(crate) fn new(path: &Path, image: DynamicImage) -> Self {
        Self {
            handle: display_handle(&image),
            width: image.width(),
            height: image.height(),
            path: path.to_path_buf(),
            image,
        }
    }
}

pub(crate) async fn decode_background<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, LoadError> + Send + 'static,
) -> Result<T, LoadError> {
    static WORKERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
    let permit = WORKERS.acquire().await.map_err(|_| LoadError::Cancelled)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    rayon::spawn(move || {
        let _permit = permit;
        if !tx.is_closed() {
            let _ = tx.send(operation());
        }
    });
    rx.await.map_err(|_| LoadError::Cancelled)?
}

pub async fn load_image(path: PathBuf) -> Result<LoadedImage, LoadError> {
    decode_background(move || {
        let format = require_format(&path)?;
        let image = decode_first(&path, format, MAX_TEX)?;
        Ok(LoadedImage::new(&path, image))
    })
    .await
}

pub async fn load_thumbnail(path: PathBuf, max_size: u32) -> Result<LoadedImage, LoadError> {
    decode_background(move || {
        let format = require_format(&path)?;
        let max_size = max_size.clamp(1, MAX_TEX);
        if format == Format::Raster(ImageFormat::Jpeg)
            && !jpeg_needs_metadata(&path)?
            && let Ok((w, h, pixels)) = decode_jpeg_scaled(&path, max_size)
        {
            let image = RgbaImage::from_raw(w, h, pixels).expect("packed JPEG pixels");
            return Ok(LoadedImage::new(&path, DynamicImage::ImageRgba8(image)));
        }
        let image = decode_first(&path, format, max_size)?;
        let rgba = color::display_rgba(&image);
        let (w, h) = rgba.dimensions();
        let rgba = if w > max_size || h > max_size {
            let (w, h, bytes) = fast_resize_rgba(rgba.as_raw(), w, h, max_size)?;
            RgbaImage::from_raw(w, h, bytes).expect("packed thumbnail pixels")
        } else {
            rgba
        };
        Ok(LoadedImage::new(&path, DynamicImage::ImageRgba8(rgba)))
    })
    .await
}

pub(crate) fn decode_first(
    path: &Path,
    format: Format,
    svg_size: u32,
) -> Result<DynamicImage, LoadError> {
    if let Some(frame) = animation::first_frame(path, format)? {
        return Ok(frame);
    }
    decode_still(path, format, svg_size)
}

pub(crate) fn decode_still(
    path: &Path,
    format: Format,
    svg_size: u32,
) -> Result<DynamicImage, LoadError> {
    match format {
        Format::Svg => load_svg_at(path, svg_size),
        Format::Jpeg2000 => native::decode(path, Some(false)),
        Format::Jpeg2000Codestream => native::decode(path, Some(true)),
        Format::Raw => native::decode(path, None),
        Format::Heif => decode_heif(path, 0),
        Format::Dib => decode_image(image::codecs::bmp::BmpDecoder::new_without_file_header(
            BufReader::new(bounded_file(path)?),
        )?),
        Format::Raster(ImageFormat::Hdr) => decode_image(
            image::codecs::hdr::HdrDecoder::new_nonstrict(BufReader::new(bounded_file(path)?))?,
        ),
        Format::JpegXl => Err(LoadError::Invalid("JPEG XL contains no image".into())),
        Format::Avif => Err(LoadError::Invalid("AVIF contains no image".into())),
        Format::Raster(ImageFormat::Tiff) => {
            pages::decode_tiff(path, 0).or_else(|_| load_raster(path, ImageFormat::Tiff))
        }
        Format::Raster(ImageFormat::Ico) => pages::decode_ico(path, 0),
        Format::Raster(format) => load_raster(path, format),
    }
}

fn jpeg_needs_metadata(path: &Path) -> Result<bool, LoadError> {
    let mut decoder = image::codecs::jpeg::JpegDecoder::new(BufReader::new(bounded_file(path)?))?;
    pixel_count(decoder.dimensions().0, decoder.dimensions().1)?;
    Ok(decoder.icc_profile()?.is_some() || decoder.orientation()? != Orientation::NoTransforms)
}

pub(crate) fn load_raster(path: &Path, format: ImageFormat) -> Result<DynamicImage, LoadError> {
    let mut reader = ImageReader::with_format(BufReader::new(bounded_file(path)?), format);
    reader.limits(decode_limits());
    decode_image(reader.into_decoder()?)
}

fn decode_image(mut decoder: impl ImageDecoder) -> Result<DynamicImage, LoadError> {
    decoder.set_limits(decode_limits())?;
    pixel_count(decoder.dimensions().0, decoder.dimensions().1)?;
    let icc = decoder.icc_profile()?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    color::to_srgb(&mut image, icc.as_deref());
    Ok(image)
}

/// Decode JPEG with DCT scaling using turbojpeg (4-8x faster than full decode)
/// This decodes directly to a smaller resolution, skipping most IDCT computation
// JPEG header/scaled dimensions are far below u32::MAX, so the usize -> u32
// casts cannot truncate.
#[allow(clippy::cast_possible_truncation)]
fn decode_jpeg_scaled(path: &Path, max_size: u32) -> Result<(u32, u32, Vec<u8>), LoadError> {
    // Read the JPEG file
    let jpeg_data = read_bounded(path)?;

    // Create decompressor
    let mut decompressor = Decompressor::new()
        .map_err(|e| LoadError::UnsupportedFormat(format!("TurboJPEG init failed: {e}")))?;

    // Read header to get original dimensions
    let header = decompressor
        .read_header(&jpeg_data)
        .map_err(|e| LoadError::UnsupportedFormat(format!("JPEG header error: {e}")))?;

    let (orig_width, orig_height) = (header.width as u32, header.height as u32);
    pixel_count(orig_width, orig_height)?;

    // Calculate the best scaling factor
    let scaling = calculate_jpeg_scale(orig_width, orig_height, max_size);

    decompressor
        .set_scaling_factor(scaling)
        .map_err(|e| LoadError::UnsupportedFormat(format!("Scale factor error: {e}")))?;

    let scaled = header.scaled(scaling);
    let width = scaled.width;
    let height = scaled.height;

    // Pre-allocate output buffer for RGBA (4 bytes per pixel)
    let mut pixels = vec![0u8; 4 * width * height];

    let mut output = Image {
        pixels: pixels.as_mut_slice(),
        width,
        pitch: 4 * width,
        height,
        format: PixelFormat::RGBA,
    };

    // Decompress with scaling directly to RGBA
    decompressor
        .decompress(&jpeg_data, output.as_deref_mut())
        .map_err(|e| LoadError::UnsupportedFormat(format!("JPEG decode error: {e}")))?;

    let width = width as u32;
    let height = height as u32;

    // If the scaled image is still larger than max_size, do a final resize
    if width > max_size || height > max_size {
        return fast_resize_rgba(&pixels, width, height, max_size);
    }

    Ok((width, height, pixels))
}

/// Calculate the best JPEG scaling factor to get close to target size
// max_dim is a JPEG dimension (<= u32::MAX) and the DCT ratios are 1/1..1/8, so
// the f32 round-trip is exact for realistic image sizes and the result is
// non-negative and in u32 range.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn calculate_jpeg_scale(width: u32, height: u32, target: u32) -> ScalingFactor {
    let max_dim = width.max(height);

    // Available scaling factors in turbojpeg (sorted largest to smallest)
    // These are the standard DCT scaling factors
    let ratios: [(usize, usize); 4] = [
        (1, 1), // 100%
        (1, 2), // 50%
        (1, 4), // 25%
        (1, 8), // 12.5%
    ];

    // Find the smallest scale that produces an image >= target size
    // (decode slightly larger, then resize down for quality)
    for &(num, denom) in ratios.iter().rev() {
        let scaled = (max_dim as f32 * num as f32 / denom as f32) as u32;
        if scaled >= target {
            return ScalingFactor::new(num, denom);
        }
    }

    // If even 1/8 is too large, use 1/8 and resize after
    ScalingFactor::ONE_EIGHTH
}

/// Decode and resize using zune, returns (width, height, `rgba_pixels`)
// Image dimensions originate from a decoded image header and are far below
// u32::MAX, so the usize -> u32 casts cannot truncate.
#[allow(clippy::cast_possible_truncation)]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn fast_resize_rgba(
    pixels: &[u8],
    src_width: u32,
    src_height: u32,
    max_size: u32,
) -> Result<(u32, u32, Vec<u8>), LoadError> {
    // Calculate target dimensions maintaining aspect ratio
    let (dst_width, dst_height) = if src_width > src_height {
        let ratio = max_size as f32 / src_width as f32;
        (max_size, (src_height as f32 * ratio).round() as u32)
    } else {
        let ratio = max_size as f32 / src_height as f32;
        ((src_width as f32 * ratio).round() as u32, max_size)
    };

    // Ensure dimensions are at least 1
    let dst_width = dst_width.max(1);
    let dst_height = dst_height.max(1);

    // Create source image from pixel data
    let src_image = FirImage::from_vec_u8(src_width, src_height, pixels.to_vec(), PixelType::U8x4)
        .map_err(|e| LoadError::UnsupportedFormat(e.to_string()))?;

    let mut dst_image = FirImage::new(dst_width, dst_height, PixelType::U8x4);

    // Resize with bilinear algorithm (good balance of speed and quality for thumbnails)
    let mut resizer = Resizer::new();
    resizer
        .resize(
            &src_image,
            &mut dst_image,
            Some(&ResizeOptions::new().resize_alg(ResizeAlg::Convolution(
                fast_image_resize::FilterType::Bilinear,
            ))),
        )
        .map_err(|e| LoadError::UnsupportedFormat(e.to_string()))?;

    Ok((dst_width, dst_height, dst_image.into_vec()))
}

/// Scale an SVG to fill `max` in its larger dimension. SVG is vector, so
/// rendering above natural size stays crisp - no pixel upscaling.
// reason: vector sizes and the rounded fit result are small, non-negative f32s.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn svg_fit_dimensions(tree: &usvg::Tree, max: u32) -> (u32, u32) {
    let size = tree.size();
    let nat_w = size.width();
    let nat_h = size.height();
    let scale = (max as f32 / nat_w).min(max as f32 / nat_h);
    let w = (nat_w * scale).round().max(1.0) as u32;
    let h = (nat_h * scale).round().max(1.0) as u32;
    (w, h)
}

/// Rasterize an SVG tree to straight-alpha RGBA at the given pixel size.
// reason: un-premultiply arithmetic stays within u8 range by construction.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn render_svg(tree: &usvg::Tree, w: u32, h: u32) -> Result<Vec<u8>, LoadError> {
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| LoadError::UnsupportedFormat("SVG: failed to create pixmap".into()))?;

    let sx = w as f32 / tree.size().width();
    let sy = h as f32 / tree.size().height();
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(sx, sy),
        &mut pixmap.as_mut(),
    );

    // tiny-skia pixels are premultiplied; `Handle::from_rgba` expects straight alpha.
    let mut pixels = pixmap.take();
    for chunk in pixels.chunks_exact_mut(4) {
        let a = u32::from(chunk[3]);
        if a > 0 && a < 255 {
            chunk[0] = ((u32::from(chunk[0]) * 255) / a).min(255) as u8;
            chunk[1] = ((u32::from(chunk[1]) * 255) / a).min(255) as u8;
            chunk[2] = ((u32::from(chunk[2]) * 255) / a).min(255) as u8;
        }
    }

    Ok(pixels)
}

fn load_svg_at(path: &Path, max: u32) -> Result<DynamicImage, LoadError> {
    let data = read_bounded(path)?;
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_string: Box::new(|_, _| None),
            resolve_data: Box::new(|mime, data, options| {
                matches!(
                    mime,
                    "image/png" | "image/jpeg" | "image/webp" | "image/gif"
                )
                .then(|| usvg::ImageHrefResolver::default_data_resolver()(mime, data, options))
                .flatten()
            }),
        },
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(&data, &options)
        .map_err(|e| LoadError::Invalid(format!("SVG: {e}")))?;
    let (width, height) = svg_fit_dimensions(&tree, max);
    let pixels = render_svg(&tree, width, height)?;
    Ok(DynamicImage::ImageRgba8(
        RgbaImage::from_raw(width, height, pixels).expect("packed SVG pixels"),
    ))
}

pub(crate) fn heif_context(path: &Path) -> Result<HeifContext<'static>, LoadError> {
    let file = bounded_file(path)?;
    let size = file.metadata()?.len();
    HeifContext::read_from_reader(Box::new(StreamReader::new(BufReader::new(file), size)))
        .map_err(|error| LoadError::Invalid(format!("HEIF: {error}")))
}

pub(crate) fn decode_heif(path: &Path, index: usize) -> Result<DynamicImage, LoadError> {
    let lib = LibHeif::new();
    let ctx = heif_context(path)?;
    let error = |error| LoadError::Invalid(format!("HEIF: {error}"));
    let primary = ctx.primary_image_handle().map_err(error)?;
    let handle = if index == 0 {
        primary
    } else {
        let id = ctx
            .image_ids()
            .into_iter()
            .filter(|id| *id != primary.item_id())
            .nth(index - 1)
            .ok_or_else(|| LoadError::Invalid("HEIF image index is out of range".into()))?;
        ctx.image_handle(id).map_err(error)?
    };
    pixel_count(handle.width(), handle.height())?;
    let chroma = if handle.luma_bits_per_pixel() > 8 {
        RgbChroma::HdrRgbaLe
    } else {
        RgbChroma::Rgba
    };
    let img = lib
        .decode(&handle, HeifColorSpace::Rgb(chroma), None)
        .map_err(error)?;
    let plane = img
        .planes()
        .interleaved
        .ok_or_else(|| LoadError::Invalid("HEIF: missing RGB pixel data".into()))?;

    let (width, height) = (img.width(), img.height());
    let count = pixel_count(width, height)?;
    let bytes_per_pixel = if chroma == RgbChroma::Rgba { 4 } else { 8 };
    let row_bytes = width as usize * bytes_per_pixel;
    let stride = plane.stride;
    if stride < row_bytes || plane.data.len() < stride.saturating_mul(height as usize) {
        return Err(LoadError::Invalid("HEIF: truncated pixel data".into()));
    }
    let rows = plane
        .data
        .chunks_exact(stride)
        .take(height as usize)
        .map(|row| &row[..row_bytes]);
    let mut image = if bytes_per_pixel == 4 {
        let mut pixels = Vec::with_capacity(count * 4);
        for row in rows {
            pixels.extend_from_slice(row);
        }
        DynamicImage::ImageRgba8(
            RgbaImage::from_raw(width, height, pixels).expect("packed HEIF pixels"),
        )
    } else {
        let depth = plane.bits_per_pixel;
        if !(9..=16).contains(&depth) {
            return Err(LoadError::Invalid(
                "HEIF: invalid high-bit-depth plane".into(),
            ));
        }
        let max = (1u32 << depth) - 1;
        let mut pixels = Vec::with_capacity(count * 4);
        for row in rows {
            pixels.extend(row.chunks_exact(2).map(|b| {
                let value = u32::from(u16::from_le_bytes([b[0], b[1]])).min(max);
                ((value * 65535 + max / 2) / max) as u16
            }));
        }
        DynamicImage::ImageRgba16(
            image::ImageBuffer::from_raw(width, height, pixels).expect("packed HEIF pixels"),
        )
    };
    let icc = handle.color_profile_raw();
    let icc = icc.as_ref().map(|profile| profile.data.as_slice());
    if let Some(nclx) = handle.color_profile_nclx() {
        let primaries = nclx.color_primaries() as u32;
        let transfer = nclx.transfer_characteristics() as u32;
        if matches!(transfer, 16 | 18) || (icc.is_none() && !matches!(primaries, 0..=2)) {
            image = DynamicImage::ImageRgba16(image.into_rgba16());
            color::convert_cicp(&mut image, icc, primaries, transfer);
        } else {
            color::to_srgb(&mut image, icc);
        }
    } else {
        color::to_srgb(&mut image, icc);
    }
    Ok(image)
}

/// Read DPI from EXIF data (JPEG/TIFF only)
// The DPI value is rounded and clamped into [0, u32::MAX] before the cast, so
// it cannot truncate or lose a sign.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[must_use]
pub fn read_dpi(path: &Path) -> Option<u32> {
    let mut file = BufReader::new(File::open(path).ok()?);
    let exif = exif::Reader::new().read_from_container(&mut file).ok()?;
    let x_res = exif.get_field(exif::Tag::XResolution, exif::In::PRIMARY)?;

    match x_res.value {
        // A malformed EXIF rational could be negative or absurdly large; round
        // and clamp into u32 range rather than letting `as` wrap silently.
        exif::Value::Rational(ref v) if !v.is_empty() => {
            Some(v[0].to_f64().round().clamp(0.0, f64::from(u32::MAX)) as u32)
        }
        _ => None,
    }
}
