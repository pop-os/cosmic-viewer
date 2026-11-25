// SPDX-License-Identifier: GPL-3.0-only

use crate::{LoadError, color, loader::pixel_count};
use image::{DynamicImage, ImageBuffer, metadata::Orientation};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    os::unix::ffi::OsStrExt,
    path::Path,
    ptr, slice,
    time::Duration,
};

#[repr(C)]
#[derive(Default)]
struct AvifInfo {
    width: u32,
    height: u32,
    count: u32,
    repetitions: c_int,
    seconds: f64,
    rotation: u32,
    mirror: u32,
    primaries: u32,
    transfer: u32,
    icc: *const u8,
    exif: *const u8,
    icc_len: usize,
    exif_len: usize,
}

#[repr(C)]
#[derive(Default)]
struct NativePixels {
    width: u32,
    height: u32,
    rgba: *mut u16,
    icc: *mut u8,
    icc_len: usize,
}

unsafe extern "C" {
    fn viewer_avif_open(path: *const c_char, error: *mut *const c_char) -> *mut c_void;
    fn viewer_avif_next(
        decoder: *mut c_void,
        info: *mut AvifInfo,
        error: *mut *const c_char,
    ) -> c_int;
    fn viewer_avif_pixels(decoder: *mut c_void, pixels: *mut u16) -> *const c_char;
    fn avifDecoderDestroy(decoder: *mut c_void);
    fn viewer_pixels_clear(pixels: *mut NativePixels);
    fn viewer_jpeg2000_decode(
        path: *const c_char,
        codestream: c_int,
        pixels: *mut NativePixels,
    ) -> *const c_char;
    fn viewer_raw_decode(path: *const c_char, pixels: *mut NativePixels) -> *const c_char;
}

fn path_cstr(path: &Path) -> Result<CString, LoadError> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| LoadError::Invalid("Invalid file path".into()))
}

fn native_error(codec: &str, error: *const c_char) -> LoadError {
    let detail = if error.is_null() {
        "unknown decoder error".into()
    } else {
        unsafe { CStr::from_ptr(error) }.to_string_lossy()
    };
    LoadError::Invalid(format!("{codec}: {detail}"))
}

pub(crate) struct AvifDecoder(ptr::NonNull<c_void>);

pub(crate) struct AvifFrame {
    pub image: DynamicImage,
    pub delay: Duration,
    pub count: u32,
    pub repetitions: Option<u32>,
}

impl AvifDecoder {
    pub fn open(path: &Path) -> Result<Self, LoadError> {
        let path = path_cstr(path)?;
        let mut error = ptr::null();
        ptr::NonNull::new(unsafe { viewer_avif_open(path.as_ptr(), &mut error) })
            .map(Self)
            .ok_or_else(|| native_error("AVIF", error))
    }

    pub fn next(&mut self) -> Result<Option<AvifFrame>, LoadError> {
        let mut info = AvifInfo::default();
        let mut error = ptr::null();
        match unsafe { viewer_avif_next(self.0.as_ptr(), &mut info, &mut error) } {
            0 => return Ok(None),
            1 => {}
            _ => return Err(native_error("AVIF", error)),
        }
        let mut pixels = vec![0; pixel_count(info.width, info.height)? * 4];
        let error = unsafe { viewer_avif_pixels(self.0.as_ptr(), pixels.as_mut_ptr()) };
        if !error.is_null() {
            return Err(native_error("AVIF", error));
        }
        let mut image = DynamicImage::ImageRgba16(
            ImageBuffer::from_raw(info.width, info.height, pixels).expect("packed AVIF frame"),
        );
        let icc = self.metadata(info.icc, info.icc_len);
        color::convert_cicp(&mut image, icc, info.primaries, info.transfer);
        if info.rotation == 0 && info.mirror == 0 {
            if let Some(exif) = self.metadata(info.exif, info.exif_len)
                && let Some(orientation) = Orientation::from_exif_chunk(exif)
            {
                image.apply_orientation(orientation);
            }
        } else {
            image.apply_orientation(match info.rotation {
                1 => Orientation::Rotate270,
                2 => Orientation::Rotate180,
                3 => Orientation::Rotate90,
                _ => Orientation::NoTransforms,
            });
            image.apply_orientation(match info.mirror {
                1 => Orientation::FlipVertical,
                2 => Orientation::FlipHorizontal,
                _ => Orientation::NoTransforms,
            });
        }
        Ok(Some(AvifFrame {
            image,
            delay: Duration::try_from_secs_f64(info.seconds).unwrap_or_default(),
            count: info.count,
            repetitions: match info.repetitions {
                -1 => None,
                n if n >= 0 => Some(n as u32 + 1),
                _ => Some(1),
            },
        }))
    }

    fn metadata(&self, data: *const u8, len: usize) -> Option<&[u8]> {
        (!data.is_null() && (1..=16 * 1024 * 1024).contains(&len))
            .then(|| unsafe { slice::from_raw_parts(data, len) })
    }
}

impl Drop for AvifDecoder {
    fn drop(&mut self) {
        unsafe { avifDecoderDestroy(self.0.as_ptr()) };
    }
}

impl Drop for NativePixels {
    fn drop(&mut self) {
        unsafe { viewer_pixels_clear(self) };
    }
}

pub(crate) fn decode(path: &Path, codestream: Option<bool>) -> Result<DynamicImage, LoadError> {
    let path = path_cstr(path)?;
    let mut output = NativePixels::default();
    let (codec, error) = match codestream {
        Some(value) => ("JPEG 2000", unsafe {
            viewer_jpeg2000_decode(path.as_ptr(), c_int::from(value), &mut output)
        }),
        None => ("Camera RAW", unsafe {
            viewer_raw_decode(path.as_ptr(), &mut output)
        }),
    };
    if !error.is_null() {
        return Err(native_error(codec, error));
    }
    let size = pixel_count(output.width, output.height)? * 4;
    if output.rgba.is_null() {
        return Err(LoadError::Invalid(format!("{codec}: missing pixel buffer")));
    }
    let pixels = unsafe { slice::from_raw_parts(output.rgba, size) }.to_vec();
    let mut image = DynamicImage::ImageRgba16(
        ImageBuffer::from_raw(output.width, output.height, pixels).expect("packed RGBA image"),
    );
    if !output.icc.is_null() && output.icc_len > 0 {
        let icc = unsafe { slice::from_raw_parts(output.icc, output.icc_len) };
        color::to_srgb(&mut image, Some(icc));
    }
    Ok(image)
}
