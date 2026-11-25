// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    LoadError, color,
    loader::{bounded_file, pixel_count},
};
use image::{DynamicImage, ImageBuffer};
use jxl_oxide::{AllocTracker, EnumColourEncoding, InitializeResult, JxlImage, RenderingIntent};
use std::{fs::File, io::Read, path::Path, time::Duration};

const INPUT_CHUNK: usize = 64 * 1024;
const MEMORY_LIMIT: usize = 256 * 1024 * 1024;

pub(crate) struct JxlDecoder {
    reader: File,
    input: Vec<u8>,
    pending: usize,
    image: JxlImage,
    index: usize,
    hdr: bool,
}

impl JxlDecoder {
    pub fn open(path: &Path) -> Result<Self, LoadError> {
        let mut reader = bounded_file(path)?;
        let mut input = vec![0; INPUT_CHUNK];
        let mut pending = 0;
        let mut uninit = JxlImage::builder()
            .alloc_tracker(AllocTracker::with_limit(MEMORY_LIMIT))
            .build_uninit();
        let mut image = loop {
            let len = read_more(&mut reader, &mut input, pending)?;
            let consumed = uninit.feed_bytes(&input[..len]).map_err(jxl_error)?;
            input.copy_within(consumed..len, 0);
            pending = len - consumed;
            match uninit.try_init().map_err(jxl_error)? {
                InitializeResult::Initialized(image) => break image,
                InitializeResult::NeedMoreData(next) => uninit = next,
            }
        };
        pixel_count(image.width(), image.height())?;
        let hdr = image.hdr_type().is_some();
        let intent = RenderingIntent::Relative;
        image.request_color_encoding(if hdr {
            EnumColourEncoding::srgb_linear(intent)
        } else {
            EnumColourEncoding::srgb(intent)
        });

        Ok(Self {
            reader,
            input,
            pending,
            image,
            index: 0,
            hdr,
        })
    }

    pub fn animation(&self) -> (Option<u32>, bool) {
        self.image
            .image_header()
            .metadata
            .animation
            .as_ref()
            .map_or((Some(1), false), |animation| {
                (
                    (animation.num_loops != 0).then_some(animation.num_loops),
                    true,
                )
            })
    }

    pub fn next(&mut self) -> Result<Option<(DynamicImage, Duration)>, LoadError> {
        while self.image.num_loaded_keyframes() <= self.index {
            if self.image.is_loading_done() {
                return Ok(None);
            }
            let len = read_more(&mut self.reader, &mut self.input, self.pending)?;
            let consumed = self
                .image
                .feed_bytes(&self.input[..len])
                .map_err(jxl_error)?;
            self.input.copy_within(consumed..len, 0);
            self.pending = len - consumed;
        }
        let render = self.image.render_frame(self.index).map_err(jxl_error)?;
        self.index += 1;
        let delay = self
            .image
            .image_header()
            .metadata
            .animation
            .as_ref()
            .map_or(Duration::ZERO, |animation| {
                let delay = f64::from(render.duration()) * f64::from(animation.tps_denominator)
                    / f64::from(animation.tps_numerator);
                Duration::try_from_secs_f64(delay).unwrap_or_default()
            });
        let mut stream = render.stream();
        let (width, height) = (stream.width(), stream.height());
        let count = pixel_count(width, height)?;
        let channels = stream.channels() as usize;
        if !matches!(channels, 3 | 4) {
            return Err(LoadError::Invalid(
                "JPEG XL could not convert the image to RGB".into(),
            ));
        }
        let mut row = vec![0.0_f32; width as usize * channels];
        let mut pixels = Vec::with_capacity(count * 4);
        for _ in 0..height {
            if stream.write_to_buffer(&mut row) != row.len() {
                return Err(LoadError::Invalid(
                    "JPEG XL has incomplete pixel data".into(),
                ));
            }
            for pixel in row.chunks_exact(channels) {
                let rgb = [pixel[0], pixel[1], pixel[2]];
                let rgb = if self.hdr {
                    color::map_linear_rgb(rgb)
                } else {
                    rgb
                };
                pixels.extend(rgb.map(quantize));
                pixels.push(quantize(pixel.get(3).copied().unwrap_or(1.0)));
            }
        }
        let image = ImageBuffer::from_raw(width, height, pixels).expect("packed JPEG XL pixels");
        Ok(Some((DynamicImage::ImageRgba16(image), delay)))
    }
}

fn quantize(value: f32) -> u16 {
    (value.clamp(0.0, 1.0) * 65535.0).round() as u16
}

fn read_more(reader: &mut File, input: &mut [u8], pending: usize) -> Result<usize, LoadError> {
    if pending == input.len() {
        return Err(LoadError::Invalid(
            "JPEG XL decoder cannot advance the input stream".into(),
        ));
    }
    let count = reader.read(&mut input[pending..])?;
    if count == 0 {
        return Err(LoadError::Invalid("Truncated JPEG XL image".into()));
    }
    Ok(pending + count)
}

fn jxl_error(error: Box<dyn std::error::Error + Send + Sync>) -> LoadError {
    LoadError::Invalid(format!("JPEG XL: {error}"))
}
