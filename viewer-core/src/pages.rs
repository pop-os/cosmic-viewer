// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    Format, LoadError, LoadedImage, color,
    loader::{bounded_file, decode_background, decode_limits, pixel_count, require_format},
};
use image::{
    DynamicImage, ImageBuffer, ImageDecoder, ImageFormat, codecs::ico::IcoDecoder,
    metadata::Orientation,
};
use std::{
    fs::File,
    io::{BufReader, Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use tiff::{
    ColorType,
    decoder::{Decoder, DecodingResult, Limits},
    tags::Tag,
};

type Tiff = Decoder<BufReader<File>>;
const MAX_TIFF_PAGES: usize = 4096;

fn tiff_error(error: tiff::TiffError) -> LoadError {
    LoadError::Invalid(format!("TIFF: {error}"))
}

fn open_tiff(path: &Path) -> Result<Tiff, LoadError> {
    let mut limits = Limits::default();
    limits.decoding_buffer_size = 256 * 1024 * 1024;
    Ok(Decoder::new(BufReader::new(bounded_file(path)?))
        .map_err(tiff_error)?
        .with_limits(limits))
}

pub(crate) fn page_count(path: &Path, format: Format) -> Result<usize, LoadError> {
    match format {
        Format::Raster(ImageFormat::Tiff) => {
            let mut decoder = open_tiff(path)?;
            let mut count = 1;
            while decoder.more_images() && count < MAX_TIFF_PAGES {
                if decoder.next_image().is_err() {
                    return Ok(count + 1);
                }
                count += 1;
            }
            Ok(count)
        }
        Format::Raster(ImageFormat::Ico) => Ok(ico_directory(&mut bounded_file(path)?)?.len()),
        Format::Heif => Ok(crate::loader::heif_context(path)?.image_ids().len().max(1)),
        _ => Ok(1),
    }
}

pub async fn load_page(path: PathBuf, index: usize) -> Result<LoadedImage, LoadError> {
    decode_background(move || {
        let image = match require_format(&path)? {
            Format::Raster(ImageFormat::Tiff) if index == 0 => crate::loader::decode_still(
                &path,
                Format::Raster(ImageFormat::Tiff),
                crate::loader::MAX_TEX,
            )?,
            Format::Raster(ImageFormat::Tiff) => decode_tiff(&path, index)?,
            Format::Raster(ImageFormat::Ico) => decode_ico(&path, index)?,
            Format::Heif => crate::loader::decode_heif(&path, index)?,
            _ => return Err(LoadError::Invalid("Image has no pages".into())),
        };
        Ok(LoadedImage::new(&path, image))
    })
    .await
}

pub(crate) fn decode_tiff(path: &Path, index: usize) -> Result<DynamicImage, LoadError> {
    let mut decoder = open_tiff(path)?;
    decoder.seek_to_image(index).map_err(tiff_error)?;
    let (width, height) = decoder.dimensions().map_err(tiff_error)?;
    pixel_count(width, height)?;
    let kind = decoder.colortype().map_err(tiff_error)?;
    let orientation = decoder
        .get_tag_unsigned::<u8>(Tag::Orientation)
        .ok()
        .and_then(Orientation::from_exif);
    let icc = decoder.get_tag_u8_vec(Tag::IccProfile).ok();
    let premultiplied = decoder
        .get_tag_u16_vec(Tag::ExtraSamples)
        .ok()
        .is_some_and(|v| v.first() == Some(&1));
    let mut pixels = DecodingResult::U8(Vec::new());
    let layout = decoder
        .read_image_to_buffer(&mut pixels)
        .map_err(tiff_error)?;
    if pixels.as_buffer(0).as_bytes().len() < layout.complete_len {
        return Err(LoadError::Limit);
    }

    let samples = match kind {
        ColorType::GrayA(_) => 2,
        ColorType::RGB(_) => 3,
        ColorType::RGBA(_) => 4,
        _ => 1,
    };
    let plane_size = |bytes_per_sample| {
        layout
            .plane_stride
            .map(|stride| stride.get() / bytes_per_sample)
    };

    macro_rules! buffer {
        ($variant:ident, $pixels:expr) => {{
            let pixels = $pixels;
            let pixels = if layout.planes > 1
                && samples > 1
                && let Some(stride) = plane_size(std::mem::size_of_val(&pixels[0]))
            {
                let count = width as usize * height as usize;
                if stride < count || (samples - 1) * stride + count > pixels.len() {
                    return Err(LoadError::Invalid(
                        "TIFF has incomplete color planes".into(),
                    ));
                }
                let mut interleaved = Vec::with_capacity(count * samples);
                for index in 0..count {
                    for channel in 0..samples {
                        interleaved.push(pixels[index + channel * stride]);
                    }
                }
                interleaved
            } else {
                pixels
            };
            ImageBuffer::from_raw(width, height, pixels).map(DynamicImage::$variant)
        }};
    }
    let mut image = match (kind, pixels) {
        (ColorType::Gray(depth @ (1 | 2 | 4)), DecodingResult::U8(pixels)) => {
            let depth = usize::from(depth);
            let width = width as usize;
            let height = height as usize;
            let row_bytes = (width * depth).div_ceil(8);
            let stride = layout
                .row_stride
                .map_or(row_bytes, std::num::NonZeroUsize::get);
            if stride < row_bytes || pixels.len() / stride < height {
                return Err(LoadError::Invalid(
                    "TIFF has incomplete grayscale rows".into(),
                ));
            }
            let levels = (1u8 << depth) - 1;
            let scale = u8::MAX / levels;
            let mut expanded = Vec::with_capacity(width * height);
            for row in pixels.chunks_exact(stride).take(height) {
                for x in 0..width {
                    let bit_offset = x * depth;
                    let sample = (row[bit_offset / 8] >> (8 - depth - bit_offset % 8)) & levels;
                    expanded.push(sample * scale);
                }
            }
            buffer!(ImageLuma8, expanded)
        }
        (ColorType::Gray(8), DecodingResult::U8(pixels)) => buffer!(ImageLuma8, pixels),
        (ColorType::GrayA(8), DecodingResult::U8(pixels)) => buffer!(ImageLumaA8, pixels),
        (ColorType::RGB(8), DecodingResult::U8(pixels)) => buffer!(ImageRgb8, pixels),
        (ColorType::RGBA(8), DecodingResult::U8(pixels)) => buffer!(ImageRgba8, pixels),
        (ColorType::Gray(16), DecodingResult::U16(pixels)) => buffer!(ImageLuma16, pixels),
        (ColorType::GrayA(16), DecodingResult::U16(pixels)) => buffer!(ImageLumaA16, pixels),
        (ColorType::RGB(16), DecodingResult::U16(pixels)) => buffer!(ImageRgb16, pixels),
        (ColorType::RGBA(16), DecodingResult::U16(pixels)) => buffer!(ImageRgba16, pixels),
        (ColorType::RGB(32), DecodingResult::F32(pixels)) => buffer!(ImageRgb32F, pixels),
        (ColorType::RGBA(32), DecodingResult::F32(pixels)) => buffer!(ImageRgba32F, pixels),
        _ => {
            return Err(LoadError::Invalid(format!(
                "TIFF page {} has an unsupported sample layout ({kind:?})",
                index + 1
            )));
        }
    }
    .ok_or_else(|| LoadError::Invalid("TIFF has invalid pixel data".into()))?;

    if premultiplied {
        macro_rules! unpremultiply {
            ($buffer:expr, $alpha:literal, $max:literal, $sample:ty) => {
                for pixel in $buffer.pixels_mut() {
                    let alpha = u32::from(pixel.0[$alpha]);
                    if alpha != 0 {
                        for value in &mut pixel.0[..$alpha] {
                            *value = ((u32::from(*value) * $max + alpha / 2) / alpha).min($max)
                                as $sample;
                        }
                    }
                }
            };
        }
        match &mut image {
            DynamicImage::ImageLumaA8(buf) => unpremultiply!(buf, 1, 255, u8),
            DynamicImage::ImageRgba8(buf) => unpremultiply!(buf, 3, 255, u8),
            DynamicImage::ImageLumaA16(buf) => unpremultiply!(buf, 1, 65535, u16),
            DynamicImage::ImageRgba16(buf) => unpremultiply!(buf, 3, 65535, u16),
            DynamicImage::ImageRgba32F(buf) => {
                for pixel in buf.pixels_mut() {
                    let alpha = pixel.0[3];
                    if alpha > 0.0 {
                        for value in &mut pixel.0[..3] {
                            *value /= alpha;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }
    if !matches!(
        image,
        DynamicImage::ImageRgba32F(_) | DynamicImage::ImageRgb32F(_)
    ) {
        color::to_srgb(&mut image, icc.as_deref());
    }
    Ok(image)
}

fn ico_directory(file: &mut File) -> Result<Vec<[u8; 16]>, LoadError> {
    let mut header = [0; 6];
    file.read_exact(&mut header)?;
    let count = u16::from_le_bytes([header[4], header[5]]);
    if header[..4] != [0, 0, 1, 0] || count == 0 {
        return Err(LoadError::Invalid("Invalid ICO directory".into()));
    }

    let mut entries = vec![[0; 16]; count.into()];
    for entry in &mut entries {
        file.read_exact(entry)?;
    }
    entries.sort_by_key(|entry| {
        let side = |value| if value == 0 { 256 } else { u32::from(value) };
        std::cmp::Reverse((
            u16::from_le_bytes([entry[6], entry[7]]),
            side(entry[0]) * side(entry[1]),
        ))
    });
    Ok(entries)
}

pub(crate) fn decode_ico(path: &Path, index: usize) -> Result<DynamicImage, LoadError> {
    let mut file = bounded_file(path)?;
    let entries = ico_directory(&mut file)?;
    let mut entry = *entries
        .get(index)
        .ok_or_else(|| LoadError::Invalid("ICO image index is out of range".into()))?;
    let size = u32::from_le_bytes(entry[8..12].try_into().expect("ICO length"));
    let offset = u32::from_le_bytes(entry[12..16].try_into().expect("ICO offset"));
    if size == 0
        || size > 16 * 1024 * 1024
        || u64::from(offset) < 6 + entries.len() as u64 * 16
        || u64::from(offset) + u64::from(size) > file.metadata()?.len()
    {
        return Err(LoadError::Invalid(
            "ICO contains an invalid image entry".into(),
        ));
    }
    entry[12..16].copy_from_slice(&22u32.to_le_bytes());
    let mut data = Vec::with_capacity(22 + size as usize);
    data.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    data.extend_from_slice(&entry);
    file.seek(SeekFrom::Start(u64::from(offset)))?;
    file.take(u64::from(size)).read_to_end(&mut data)?;
    let mut decoder = IcoDecoder::new(Cursor::new(data))?;
    decoder.set_limits(decode_limits())?;
    Ok(DynamicImage::from_decoder(decoder)?)
}
