// SPDX-License-Identifier: GPL-3.0-only

use crate::{LoadError, color, output_format};
use image::{
    DynamicImage, ImageEncoder, ImageFormat,
    codecs::pnm::{PnmEncoder, PnmSubtype, SampleEncoding},
};
use std::{fs, io::BufWriter, path::Path};

pub fn save_image(image: DynamicImage, path: &Path) -> Result<(), LoadError> {
    let format = output_format(path).ok_or_else(|| {
        LoadError::UnsupportedFormat("Choose a supported output extension".into())
    })?;
    let path = if path.is_symlink() {
        fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    if let Ok(metadata) = path.metadata() {
        if !metadata.is_file() {
            return Err(LoadError::Invalid(
                "The destination is not a regular file".into(),
            ));
        }
        temp.as_file().set_permissions(metadata.permissions())?;
    }
    let image = match (&image, format) {
        (
            DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_),
            ImageFormat::Hdr | ImageFormat::OpenExr | ImageFormat::Tiff,
        ) => image,
        (DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_), _) => {
            DynamicImage::ImageRgba8(color::display_rgba(&image))
        }
        _ => image,
    };
    let image = match format {
        ImageFormat::Jpeg => DynamicImage::ImageRgb8(image.into_rgb8()),
        ImageFormat::Gif
        | ImageFormat::WebP
        | ImageFormat::Ico
        | ImageFormat::Qoi
        | ImageFormat::Tga
        | ImageFormat::Avif
        | ImageFormat::Bmp => DynamicImage::ImageRgba8(image.into_rgba8()),
        ImageFormat::Farbfeld => DynamicImage::ImageRgba16(image.into_rgba16()),
        ImageFormat::Hdr => DynamicImage::ImageRgb32F(image.into_rgb32f()),
        ImageFormat::OpenExr => DynamicImage::ImageRgba32F(image.into_rgba32f()),
        _ => image,
    };
    {
        use std::io::Write as _;
        let mut writer = BufWriter::new(temp.as_file_mut());
        if format == ImageFormat::Pnm {
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let binary = SampleEncoding::Binary;
            let (subtype, image) = match extension.as_str() {
                "pbm" => {
                    let mut luma = image.into_luma8();
                    for sample in luma.as_flat_samples_mut().samples {
                        *sample = u8::from(*sample >= 128);
                    }
                    (PnmSubtype::Bitmap(binary), DynamicImage::ImageLuma8(luma))
                }
                "pgm" => (
                    PnmSubtype::Graymap(binary),
                    DynamicImage::ImageLuma8(image.into_luma8()),
                ),
                "ppm" => (
                    PnmSubtype::Pixmap(binary),
                    DynamicImage::ImageRgb8(image.into_rgb8()),
                ),
                _ => (PnmSubtype::ArbitraryMap, image),
            };
            PnmEncoder::new(&mut writer)
                .with_subtype(subtype)
                .write_image(
                    image.as_bytes(),
                    image.width(),
                    image.height(),
                    image.color().into(),
                )?;
        } else {
            image.write_to(&mut writer, format)?;
        }
        writer.flush()?;
    }
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}
