// SPDX-License-Identifier: GPL-3.0-only

use crate::{LoadError, color, loader::pixel_count};
use image::{DynamicImage, ImageBuffer, Pixel, Rgba, metadata::Orientation};
use png::{AnimationControl, BlendOp, ColorType, DisposeOp, FrameControl};
use std::{fs::File, io::BufReader, time::Duration};

type Canvas = ImageBuffer<Rgba<u16>, Vec<u16>>;

pub(crate) struct Apng16 {
    reader: png::Reader<BufReader<File>>,
    timing: AnimationControl,
    remaining: u32,
    canvas: Canvas,
    raw: Vec<u8>,
    icc: Option<Vec<u8>>,
    orientation: Orientation,
}

impl Apng16 {
    pub fn open(
        file: BufReader<File>,
        icc: Option<Vec<u8>>,
        orientation: Orientation,
    ) -> Result<Self, LoadError> {
        let mut decoder = png::Decoder::new(file);
        decoder.set_transformations(png::Transformations::EXPAND);
        decoder.set_limits(png::Limits {
            bytes: 128 * 1024 * 1024,
        });
        let mut reader = decoder.read_info().map_err(png_error)?;
        let info = reader.info();
        if pixel_count(info.width, info.height)? > 32 * 1024 * 1024 {
            return Err(LoadError::Limit);
        }
        let canvas = Canvas::new(info.width, info.height);
        let timing = *info.animation_control().ok_or_else(bad_frame)?;
        let mut raw = vec![0; reader.output_buffer_size().ok_or(LoadError::Limit)?];

        if info.frame_control().is_none() {
            reader.next_frame(&mut raw).map_err(png_error)?;
        }

        Ok(Self {
            reader,
            timing,
            remaining: timing.num_frames,
            canvas,
            raw,
            icc,
            orientation,
        })
    }

    pub fn timing(&self) -> (Option<u32>, u32) {
        let plays = self.timing.num_plays;
        ((plays != 0).then_some(plays), self.timing.num_frames)
    }

    pub fn next(&mut self) -> Result<Option<(DynamicImage, Duration)>, LoadError> {
        if self.remaining == 0 {
            return Ok(None);
        }

        let output = self.reader.next_frame(&mut self.raw).map_err(png_error)?;
        let frame = *self.reader.info().frame_control().ok_or_else(bad_frame)?;
        if output.bit_depth != png::BitDepth::Sixteen
            || output.width != frame.width
            || output.height != frame.height
            || frame.x_offset.saturating_add(frame.width) > self.canvas.width()
            || frame.y_offset.saturating_add(frame.height) > self.canvas.height()
        {
            return Err(bad_frame());
        }

        let mut previous = Vec::new();
        if frame.dispose_op == DisposeOp::Previous {
            previous.reserve(frame.width as usize * frame.height as usize);
        }
        let bytes = output.color_type.samples() * 2;
        let stride = if self.reader.info().interlaced {
            self.canvas.width() as usize * bytes
        } else {
            output.line_size
        };
        let pixels = self
            .raw
            .chunks_exact(stride)
            .take(frame.height as usize)
            .flat_map(|row| row[..frame.width as usize * bytes].chunks_exact(bytes));
        for (index, pixel) in pixels.enumerate() {
            let (x, y) = position(&frame, index);
            let destination = self.canvas.get_pixel_mut(x, y);
            if frame.dispose_op == DisposeOp::Previous {
                previous.push(*destination);
            }
            let sample =
                |index: usize| u16::from_be_bytes([pixel[index * 2], pixel[index * 2 + 1]]);
            let source = Rgba(match output.color_type {
                ColorType::Grayscale => [sample(0), sample(0), sample(0), u16::MAX],
                ColorType::GrayscaleAlpha => [sample(0), sample(0), sample(0), sample(1)],
                ColorType::Rgb => [sample(0), sample(1), sample(2), u16::MAX],
                ColorType::Rgba => [sample(0), sample(1), sample(2), sample(3)],
                ColorType::Indexed => return Err(bad_frame()),
            });
            match frame.blend_op {
                BlendOp::Source => *destination = source,
                BlendOp::Over => destination.blend(&source),
            }
        }

        let mut output = DynamicImage::ImageRgba16(self.canvas.clone());
        match frame.dispose_op {
            DisposeOp::None => {}
            DisposeOp::Background => {
                for y in frame.y_offset..frame.y_offset + frame.height {
                    for x in frame.x_offset..frame.x_offset + frame.width {
                        self.canvas.put_pixel(x, y, Rgba([0; 4]));
                    }
                }
            }
            DisposeOp::Previous => {
                for (index, pixel) in previous.into_iter().enumerate() {
                    let (x, y) = position(&frame, index);
                    self.canvas.put_pixel(x, y, pixel);
                }
            }
        }
        output.apply_orientation(self.orientation);
        color::to_srgb(&mut output, self.icc.as_deref());
        self.remaining -= 1;
        let denominator = if frame.delay_den == 0 {
            100
        } else {
            frame.delay_den
        };
        let delay = Duration::from_secs_f64(f64::from(frame.delay_num) / f64::from(denominator));
        Ok(Some((output, delay)))
    }
}

fn position(frame: &FrameControl, index: usize) -> (u32, u32) {
    let index = index as u32;
    (
        frame.x_offset + index % frame.width,
        frame.y_offset + index / frame.width,
    )
}

fn bad_frame() -> LoadError {
    LoadError::Invalid("APNG has an invalid frame layout".into())
}

fn png_error(error: png::DecodingError) -> LoadError {
    LoadError::Invalid(format!("APNG: {error}"))
}
