use cosmic::widget::image::Handle;
use fast_image_resize::{PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image as FirImage};
use image::{DynamicImage, RgbaImage};
use std::{
    fmt::{self, Debug, Formatter},
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use thiserror::Error;
use turbojpeg::{Decompressor, Image, PixelFormat, ScalingFactor};
use zune_image::codecs::bmp::zune_core::colorspace::ColorSpace;
use zune_image::image::Image as ZuneImage;

// Temporary fix until I can get a patch into upstream
const MAX_TEX: u32 = 2048;

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("Failed to read file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to decode image: {0}")]
    Decode(#[from] image::ImageError),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
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

// Display texture, downscaled to MAX_TEX. The source image is left full-res.
fn display_handle(image: &DynamicImage) -> Handle {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    if (width > MAX_TEX || height > MAX_TEX)
        && let Ok((tw, th, pixels)) = fast_resize_rgba(rgba.as_raw(), width, height, MAX_TEX)
    {
        return Handle::from_rgba(tw, th, pixels);
    }
    Handle::from_rgba(width, height, rgba.into_raw())
}

/// Decode the image at `path` on a background thread.
///
/// # Errors
///
/// Returns [`LoadError`] if the file cannot be read, the format is unsupported
/// or fails to decode, or the decode task is cancelled before completion.
pub async fn load_image(path: PathBuf) -> Result<LoadedImage, LoadError> {
    let (tx, rx) = tokio::sync::oneshot::channel();

    rayon::spawn(move || {
        let result = load_image_sync(&path);
        let _ = tx.send(result);
    });

    rx.await.map_err(|_| LoadError::Cancelled)?
}

fn load_image_sync(path: &Path) -> Result<LoadedImage, LoadError> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();

    // Use turbojpeg for JPEGs (faster than zune/image crate)
    if matches!(extension.as_str(), "jpg" | "jpeg")
        && let Ok(img) = load_jpeg_full(path)
    {
        return Ok(img);
    }
    // Fall through to other decoders if turbojpeg fails

    if is_zune_supported(&extension) {
        match load_with_zune(path) {
            Ok(img) => return Ok(img),
            Err(_) => {
                return load_with_image(path);
            }
        }
    }

    // Standard image formats via the 'image' crate
    let img = image::open(path)?;
    let (width, height) = (img.width(), img.height());
    let handle = display_handle(&img);

    Ok(LoadedImage {
        handle,
        image: img,
        width,
        height,
        path: path.to_path_buf(),
    })
}

/// Load full JPEG using turbojpeg (faster than zune/image crate)
// JPEG header dimensions are far below u32::MAX, so the usize -> u32 casts
// cannot truncate.
#[allow(clippy::cast_possible_truncation)]
fn load_jpeg_full(path: &Path) -> Result<LoadedImage, LoadError> {
    let mut file = File::open(path)?;
    let mut jpeg_data = Vec::new();
    file.read_to_end(&mut jpeg_data)?;

    let mut decompressor = Decompressor::new()
        .map_err(|e| LoadError::UnsupportedFormat(format!("TurboJPEG init failed: {e}")))?;

    let header = decompressor
        .read_header(&jpeg_data)
        .map_err(|e| LoadError::UnsupportedFormat(format!("JPEG header error: {e}")))?;

    let width = header.width;
    let height = header.height;

    // Pre-allocate output buffer for RGBA (4 bytes per pixel)
    let mut pixels = vec![0u8; 4 * width * height];

    let mut output = Image {
        pixels: pixels.as_mut_slice(),
        width,
        pitch: 4 * width,
        height,
        format: PixelFormat::RGBA,
    };

    decompressor
        .decompress(&jpeg_data, output.as_deref_mut())
        .map_err(|e| LoadError::UnsupportedFormat(format!("JPEG decode error: {e}")))?;

    let rgba_image = RgbaImage::from_raw(width as u32, height as u32, pixels)
        .expect("pixel buffer matches dimensions");
    let image = DynamicImage::ImageRgba8(rgba_image);
    let handle = display_handle(&image);

    Ok(LoadedImage {
        handle,
        image,
        width: width as u32,
        height: height as u32,
        path: path.to_path_buf(),
    })
}

fn is_zune_supported(extension: &str) -> bool {
    matches!(
        extension,
        "jpg"
            | "jpeg"
            | "png"
            | "ppm"
            | "pgm"
            | "pbm"
            | "pnm"
            | "bmp"
            | "qoi"
            | "ff"
            | "farbfeld"
            | "hdr"
            | "jxl"
    )
}

// Image dimensions originate from a decoded image header and are far below
// u32::MAX, so the usize -> u32 casts cannot truncate.
#[allow(clippy::cast_possible_truncation)]
fn load_with_zune(path: &Path) -> Result<LoadedImage, LoadError> {
    let mut img = ZuneImage::open(path).map_err(|e| LoadError::UnsupportedFormat(e.to_string()))?;

    img.convert_color(ColorSpace::RGBA)
        .map_err(|e| LoadError::UnsupportedFormat(e.to_string()))?;

    let (width, height) = img.dimensions();

    let pixels = img
        .flatten_to_u8()
        .into_iter()
        .next()
        .ok_or_else(|| LoadError::UnsupportedFormat("No pixel data".into()))?;

    let rgba_image = RgbaImage::from_raw(width as u32, height as u32, pixels)
        .expect("pixel buffer matches dimensions");
    let image = DynamicImage::ImageRgba8(rgba_image);
    let handle = display_handle(&image);

    Ok(LoadedImage {
        handle,
        image,
        width: width as u32,
        height: height as u32,
        path: path.to_path_buf(),
    })
}

fn load_with_image(path: &Path) -> Result<LoadedImage, LoadError> {
    let img = image::open(path)?;
    let (width, height) = (img.width(), img.height());
    let handle = display_handle(&img);

    Ok(LoadedImage {
        handle,
        image: img,
        width,
        height,
        path: path.to_path_buf(),
    })
}

/// Decode the image at `path` and downscale it to fit `max_size` on its longest
/// edge, on a background thread.
///
/// # Errors
///
/// Returns [`LoadError`] if the file cannot be read, the format is unsupported
/// or fails to decode, or the decode task is cancelled before completion.
pub async fn load_thumbnail(path: PathBuf, max_size: u32) -> Result<LoadedImage, LoadError> {
    let (tx, rx) = tokio::sync::oneshot::channel();

    rayon::spawn(move || {
        let result = load_thumbnail_sync(&path, max_size);
        let _ = tx.send(result);
    });

    rx.await.map_err(|_| LoadError::Cancelled)?
}

fn load_thumbnail_sync(path: &Path, max_size: u32) -> Result<LoadedImage, LoadError> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();

    // 1. For JPEGs, try EXIF thumbnail extraction (very fast, no full decode)
    if matches!(extension.as_str(), "jpg" | "jpeg") {
        if let Ok((width, height, pixels)) = extract_exif_thumbnail(path, max_size) {
            let handle = Handle::from_rgba(width, height, pixels.clone());

            let rgba_image = RgbaImage::from_raw(width, height, pixels)
                .expect("pixel buffer matches dimensions");
            let dynamic_image = DynamicImage::ImageRgba8(rgba_image);

            return Ok(LoadedImage {
                handle,
                image: dynamic_image,
                width,
                height,
                path: path.to_path_buf(),
            });
        }

        // 2. For JPEGs without EXIF, use turbojpeg with DCT scaling (4-8x faster)
        if let Ok((width, height, pixels)) = decode_jpeg_scaled(path, max_size) {
            let handle = Handle::from_rgba(width, height, pixels.clone());

            let rgba_image = RgbaImage::from_raw(width, height, pixels)
                .expect("pixel buffer matches dimensions");
            let dynamic_image = DynamicImage::ImageRgba8(rgba_image);

            return Ok(LoadedImage {
                handle,
                image: dynamic_image,
                width,
                height,
                path: path.to_path_buf(),
            });
        }
    }

    // 3. Fall back to full decode + resize (non-JPEGs or if turbojpeg fails)
    let (width, height, pixels) = if is_zune_supported(&extension) {
        match decode_and_resize_zune(path, max_size) {
            Ok(result) => result,
            Err(_) => decode_and_resize_image(path, max_size)?,
        }
    } else {
        decode_and_resize_image(path, max_size)?
    };

    let handle = Handle::from_rgba(width, height, pixels.clone());

    let rgba_image =
        RgbaImage::from_raw(width, height, pixels).expect("pixel buffer matches dimensions");
    let dynamic_image = DynamicImage::ImageRgba8(rgba_image);

    Ok(LoadedImage {
        handle,
        image: dynamic_image,
        width,
        height,
        path: path.to_path_buf(),
    })
}

/// Extract embedded EXIF thumbnail from JPEG files
/// This is extremely fast as it only reads a small portion of the file
fn extract_exif_thumbnail(path: &Path, max_size: u32) -> Result<(u32, u32, Vec<u8>), LoadError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let exif = exif::Reader::new()
        .read_from_container(&mut reader)
        .map_err(|e| LoadError::UnsupportedFormat(format!("No EXIF data: {e}")))?;

    // Get the thumbnail data
    let thumbnail = exif
        .get_field(exif::Tag::JPEGInterchangeFormat, exif::In::THUMBNAIL).zip(exif.get_field(exif::Tag::JPEGInterchangeFormatLength, exif::In::THUMBNAIL));

    if thumbnail.is_none() {
        return Err(LoadError::UnsupportedFormat("No EXIF thumbnail".into()));
    }

    // The exif crate doesn't directly expose thumbnail bytes, so we need to
    // re-read the file and extract the thumbnail using the offset/length
    let thumb_bytes = extract_thumbnail_bytes(path, &exif)?;

    // Decode the embedded JPEG thumbnail
    let img = image::load_from_memory_with_format(&thumb_bytes, image::ImageFormat::Jpeg)
        .map_err(LoadError::Decode)?;

    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    // If thumbnail is already small enough, return it
    if width <= max_size && height <= max_size {
        return Ok((width, height, rgba.into_raw()));
    }

    // Resize if thumbnail is larger than requested
    let pixels = rgba.into_raw();
    fast_resize_rgba(&pixels, width, height, max_size)
}

/// Extract raw thumbnail bytes from JPEG using EXIF offset/length
fn extract_thumbnail_bytes(path: &Path, exif: &exif::Exif) -> Result<Vec<u8>, LoadError> {
    let offset = exif
        .get_field(exif::Tag::JPEGInterchangeFormat, exif::In::THUMBNAIL)
        .and_then(|f| f.value.get_uint(0))
        .ok_or_else(|| LoadError::UnsupportedFormat("No thumbnail offset".into()))?;

    let length = exif
        .get_field(exif::Tag::JPEGInterchangeFormatLength, exif::In::THUMBNAIL)
        .and_then(|f| f.value.get_uint(0))
        .ok_or_else(|| LoadError::UnsupportedFormat("No thumbnail length".into()))?;

    // Sanity check - thumbnails shouldn't be huge
    if length > 1_000_000 {
        return Err(LoadError::UnsupportedFormat("Thumbnail too large".into()));
    }

    let mut file = File::open(path)?;

    // EXIF data starts after APP1 marker, typically at offset 12 from file start
    // The offset in EXIF is relative to the TIFF header, which is inside APP1
    // We need to find the actual file offset

    // Read the APP1 segment to find the TIFF header offset
    let mut header = [0u8; 12];
    file.read_exact(&mut header)?;

    // JPEG starts with FFD8, then APP1 marker FFE1, then 2-byte length, then "Exif\0\0"
    // TIFF header starts after "Exif\0\0"
    let tiff_offset = if &header[0..2] == b"\xFF\xD8" && &header[2..4] == b"\xFF\xE1" {
        // APP1 starts at offset 2, length is at 4-5, "Exif\0\0" is at 6-11
        // TIFF header is at offset 12
        12u64
    } else {
        // Fallback: scan for APP1 marker
        file.seek(SeekFrom::Start(0))?;
        find_tiff_header_offset(&mut file)?
    };

    // Seek to thumbnail position (TIFF header offset + thumbnail offset in EXIF)
    file.seek(SeekFrom::Start(tiff_offset + u64::from(offset)))?;

    let mut thumb_data = vec![0u8; length as usize];
    file.read_exact(&mut thumb_data)?;

    Ok(thumb_data)
}

/// Scan JPEG file to find the TIFF header offset within APP1
fn find_tiff_header_offset(file: &mut File) -> Result<u64, LoadError> {
    file.seek(SeekFrom::Start(0))?;

    let mut marker = [0u8; 2];
    file.read_exact(&mut marker)?;

    if marker != [0xFF, 0xD8] {
        return Err(LoadError::UnsupportedFormat("Not a JPEG file".into()));
    }

    loop {
        file.read_exact(&mut marker)?;

        if marker[0] != 0xFF {
            return Err(LoadError::UnsupportedFormat("Invalid JPEG marker".into()));
        }

        // Skip padding FF bytes
        while marker[1] == 0xFF {
            file.read_exact(&mut marker[1..2])?;
        }

        // Read segment length
        let mut len_bytes = [0u8; 2];
        file.read_exact(&mut len_bytes)?;
        let segment_len = u64::from(u16::from_be_bytes(len_bytes));

        if marker[1] == 0xE1 {
            // APP1 segment - check for EXIF
            let mut exif_header = [0u8; 6];
            file.read_exact(&mut exif_header)?;

            if &exif_header[0..4] == b"Exif" {
                // TIFF header starts here
                return Ok(file.stream_position()?);
            }

            // Non-EXIF APP1 - already consumed 6 bytes after len_bytes,
            // so remaining = segment_len - 2 (len) - 6 (header read) = segment_len - 8
            let current_pos = file.stream_position()?;
            file.seek(SeekFrom::Start(current_pos + segment_len - 8))?;
            continue;
        }

        // Skip to next segment
        let current_pos = file.stream_position()?;
        file.seek(SeekFrom::Start(current_pos + segment_len - 2))?;

        // End of image
        if marker[1] == 0xD9 {
            break;
        }
    }

    Err(LoadError::UnsupportedFormat(
        "No EXIF APP1 segment found".into(),
    ))
}

/// Decode JPEG with DCT scaling using turbojpeg (4-8x faster than full decode)
/// This decodes directly to a smaller resolution, skipping most IDCT computation
// JPEG header/scaled dimensions are far below u32::MAX, so the usize -> u32
// casts cannot truncate.
#[allow(clippy::cast_possible_truncation)]
fn decode_jpeg_scaled(path: &Path, max_size: u32) -> Result<(u32, u32, Vec<u8>), LoadError> {

    // Read the JPEG file
    let mut file = File::open(path)?;
    let mut jpeg_data = Vec::new();
    file.read_to_end(&mut jpeg_data)?;

    // Create decompressor
    let mut decompressor = Decompressor::new()
        .map_err(|e| LoadError::UnsupportedFormat(format!("TurboJPEG init failed: {e}")))?;

    // Read header to get original dimensions
    let header = decompressor
        .read_header(&jpeg_data)
        .map_err(|e| LoadError::UnsupportedFormat(format!("JPEG header error: {e}")))?;

    let (orig_width, orig_height) = (header.width as u32, header.height as u32);

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
    // (we want to decode slightly larger, then resize down for quality)
    for &(num, denom) in &ratios {
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
fn decode_and_resize_zune(path: &Path, max_size: u32) -> Result<(u32, u32, Vec<u8>), LoadError> {
    let mut img = ZuneImage::open(path).map_err(|e| LoadError::UnsupportedFormat(e.to_string()))?;

    img.convert_color(ColorSpace::RGBA)
        .map_err(|e| LoadError::UnsupportedFormat(e.to_string()))?;

    let (width, height) = img.dimensions();

    let pixels = img
        .flatten_to_u8()
        .into_iter()
        .next()
        .ok_or_else(|| LoadError::UnsupportedFormat("No pixel data".into()))?;

    // If already small enough, return directly
    if width <= max_size as usize && height <= max_size as usize {
        return Ok((width as u32, height as u32, pixels));
    }

    fast_resize_rgba(&pixels, width as u32, height as u32, max_size)
}

/// Decode and resize using image crate, returns (width, height, `rgba_pixels`)
fn decode_and_resize_image(path: &Path, max_size: u32) -> Result<(u32, u32, Vec<u8>), LoadError> {
    let img = image::open(path)?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    // If already small enough, return directly
    if width <= max_size && height <= max_size {
        return Ok((width, height, rgba.into_raw()));
    }

    let pixels = rgba.into_raw();
    fast_resize_rgba(&pixels, width, height, max_size)
}

/// Fast RGBA image resize using SIMD-optimized `fast_image_resize` crate
// Source dimensions are positive image sizes and `ratio` is in (0, 1], so the
// scaled product is non-negative and within u32 range after rounding; the f32
// round-trip is exact for realistic image sizes.
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
