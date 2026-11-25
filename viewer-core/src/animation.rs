// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    Format, LoadError, LoadedImage,
    apng::Apng16,
    color,
    jxl::JxlDecoder,
    loader::{MAX_TEX, bounded_file, decode_limits, decode_still, pixel_count, require_format},
    native::AvifDecoder,
    pages,
};
use image::{
    AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat,
    codecs::{gif::GifDecoder, png::PngDecoder, webp::WebPDecoder},
    metadata::{LoopCount, Orientation},
};
use std::{
    io::BufReader,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{Mutex, Semaphore, mpsc, oneshot};

const MIN_FRAME_DELAY: Duration = Duration::from_millis(10);
static DECODE_SLOTS: Semaphore = Semaphore::const_new(2);

#[derive(Debug, Clone)]
pub struct AnimationFrame {
    pub image: LoadedImage,
    pub delay: Duration,
}

#[derive(Debug, Clone)]
pub struct Animation {
    pub first_delay: Duration,
    frames: Arc<Mutex<mpsc::Receiver<Result<AnimationFrame, LoadError>>>>,
}

impl Animation {
    pub async fn next(&self) -> Option<Result<AnimationFrame, LoadError>> {
        self.frames.lock().await.recv().await
    }
}

#[derive(Debug)]
pub struct OpenedImage {
    pub image: LoadedImage,
    pub format: Format,
    pub page_count: usize,
    pub animation: Option<Animation>,
}

struct Source {
    frames: Box<dyn Iterator<Item = Result<(DynamicImage, Duration), LoadError>>>,
    plays: Option<u32>,
    frame_count: Option<u32>,
}

fn raster_frames(
    decoder: impl AnimationDecoder<'static>,
    icc: Option<Vec<u8>>,
    orientation: Orientation,
) -> Source {
    let plays = match decoder.loop_count() {
        LoopCount::Finite(count) => Some(count.get()),
        LoopCount::Infinite => None,
    };
    let frames = decoder.into_frames().map(move |frame| {
        let frame = frame?;
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let mut image = DynamicImage::ImageRgba8(frame.into_buffer());
        image.apply_orientation(orientation);
        color::to_srgb(&mut image, icc.as_deref());
        Ok((
            image,
            Duration::from_secs_f64(f64::from(numerator) / f64::from(denominator) / 1000.0),
        ))
    });
    Source {
        frames: Box::new(frames),
        plays,
        frame_count: None,
    }
}

fn source(path: &Path, format: Format) -> Result<Option<Source>, LoadError> {
    let reader = || bounded_file(path).map(BufReader::new);
    let stream = match format {
        Format::Raster(ImageFormat::Gif) => {
            let mut decoder = GifDecoder::new(reader()?)?;
            pixel_count(decoder.dimensions().0, decoder.dimensions().1)?;
            decoder.set_limits(decode_limits())?;
            let icc = decoder.icc_profile()?;
            raster_frames(decoder, icc, Orientation::NoTransforms)
        }
        Format::Raster(ImageFormat::Png) => {
            let mut decoder = PngDecoder::with_limits(reader()?, decode_limits())?;
            if !decoder.is_apng()? {
                return Ok(None);
            }
            pixel_count(decoder.dimensions().0, decoder.dimensions().1)?;
            let icc = decoder.icc_profile()?;
            let orientation = decoder.orientation()?;
            if decoder.color_type().bits_per_pixel()
                / u16::from(decoder.color_type().channel_count())
                == 16
            {
                let mut decoder = Apng16::open(reader()?, icc, orientation)?;
                let (plays, count) = decoder.timing();
                let frames = std::iter::from_fn(move || decoder.next().transpose());
                Source {
                    frames: Box::new(frames),
                    plays,
                    frame_count: Some(count),
                }
            } else {
                raster_frames(decoder.apng()?, icc, orientation)
            }
        }
        Format::Raster(ImageFormat::WebP) => {
            let mut decoder = WebPDecoder::new(reader()?)?;
            if !decoder.has_animation() {
                return Ok(None);
            }
            pixel_count(decoder.dimensions().0, decoder.dimensions().1)?;
            decoder.set_limits(decode_limits())?;
            let icc = decoder.icc_profile()?;
            let orientation = decoder.orientation()?;
            raster_frames(decoder, icc, orientation)
        }
        Format::JpegXl => {
            let mut decoder = JxlDecoder::open(path)?;
            let (plays, is_animated) = decoder.animation();
            let frames = std::iter::from_fn(move || decoder.next().transpose());
            Source {
                frames: Box::new(frames),
                plays,
                frame_count: (!is_animated).then_some(1),
            }
        }
        Format::Avif => {
            let mut decoder = AvifDecoder::open(path)?;
            let first = decoder.next()?.ok_or_else(no_frames)?;
            let frames = std::iter::once(Ok((first.image, first.delay))).chain(std::iter::from_fn(
                move || {
                    decoder
                        .next()
                        .transpose()
                        .map(|result| result.map(|frame| (frame.image, frame.delay)))
                },
            ));
            Source {
                frames: Box::new(frames),
                plays: first.repetitions,
                frame_count: Some(first.count),
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(stream))
}

fn no_frames() -> LoadError {
    LoadError::Invalid("Image contains no decodable frames".into())
}

pub(crate) fn first_frame(path: &Path, format: Format) -> Result<Option<DynamicImage>, LoadError> {
    source(path, format)?
        .map(|mut source| {
            source
                .frames
                .next()
                .ok_or_else(no_frames)?
                .map(|(image, _)| image)
        })
        .transpose()
}

pub async fn open_image(path: PathBuf) -> Result<OpenedImage, LoadError> {
    let permit = DECODE_SLOTS
        .acquire()
        .await
        .map_err(|_| LoadError::Cancelled)?;
    let (tx, rx) = oneshot::channel();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if !tx.is_closed() {
            decode(path, tx);
        }
    });
    rx.await.map_err(|_| LoadError::Cancelled)?
}

fn decode(path: PathBuf, available: oneshot::Sender<Result<OpenedImage, LoadError>>) {
    let opened = (|| {
        let format = require_format(&path)?;
        let Some(mut stream) = source(&path, format)? else {
            let image = decode_still(&path, format, MAX_TEX)?;
            let page_count = pages::page_count(&path, format)?;
            return Ok((
                OpenedImage {
                    image: LoadedImage::new(&path, image),
                    format,
                    page_count,
                    animation: None,
                },
                None,
            ));
        };
        let (first, delay) = stream.frames.next().ok_or_else(no_frames)??;
        let image = LoadedImage::new(&path, first);
        let next = if stream.frame_count == Some(1) {
            None
        } else {
            stream.frames.next()
        };
        let Some(next) = next else {
            return Ok((
                OpenedImage {
                    image,
                    format,
                    page_count: 1,
                    animation: None,
                },
                None,
            ));
        };
        let (sender, receiver) = mpsc::channel(1);
        let animation = Animation {
            frames: Arc::new(Mutex::new(receiver)),
            first_delay: delay.max(MIN_FRAME_DELAY),
        };
        Ok((
            OpenedImage {
                image,
                format,
                page_count: 1,
                animation: Some(animation),
            },
            Some((sender, stream, next)),
        ))
    })();

    match opened {
        Ok((image, stream)) => {
            let format = image.format;
            if available.send(Ok(image)).is_ok()
                && let Some((sender, stream, next)) = stream
            {
                play(&path, stream, next, format, &sender);
            }
        }
        Err(error) => {
            let _ = available.send(Err(error));
        }
    }
}

fn play(
    path: &Path,
    mut stream: Source,
    next: Result<(DynamicImage, Duration), LoadError>,
    format: Format,
    sender: &mpsc::Sender<Result<AnimationFrame, LoadError>>,
) {
    let push = |frame: Result<(DynamicImage, Duration), LoadError>| {
        let result = frame.map(|(image, delay): (DynamicImage, Duration)| AnimationFrame {
            image: LoadedImage::new(path, image),
            delay: delay.max(MIN_FRAME_DELAY),
        });
        let valid = result.is_ok();
        sender.blocking_send(result).is_ok() && valid
    };
    if !push(next) {
        return;
    }
    let mut completed = 0_u32;
    let mut max_plays = stream.plays;
    loop {
        while !sender.is_closed() {
            let Some(frame) = stream.frames.next() else {
                break;
            };
            if !push(frame) {
                return;
            }
        }
        completed = completed.saturating_add(1);
        if sender.is_closed() {
            return;
        }
        if completed == 1 && format == Format::Raster(ImageFormat::Gif) {
            match gif_plays(path, sender) {
                Ok(plays) => max_plays = plays,
                Err(error) => {
                    let _ = sender.blocking_send(Err(error));
                    return;
                }
            }
        }
        if max_plays.is_some_and(|plays| completed >= plays) {
            return;
        }
        match source(path, format) {
            Ok(Some(replay)) => stream = replay,
            Ok(None) => return,
            Err(error) => {
                let _ = sender.blocking_send(Err(error));
                return;
            }
        }
    }
}

fn gif_plays(
    path: &Path,
    sender: &mpsc::Sender<Result<AnimationFrame, LoadError>>,
) -> Result<Option<u32>, LoadError> {
    let error = |error| LoadError::Invalid(format!("GIF: {error}"));
    let mut options = gif::DecodeOptions::new();
    options.skip_frame_decoding(true);
    let mut decoder = options.read_info(bounded_file(path)?).map_err(error)?;
    while decoder.repeat() == gif::Repeat::Finite(0) && !sender.is_closed() {
        if decoder.read_next_frame().map_err(error)?.is_none() {
            break;
        }
    }
    Ok(match decoder.repeat() {
        gif::Repeat::Infinite => None,
        gif::Repeat::Finite(repeats) => Some(u32::from(repeats) + 1),
    })
}
