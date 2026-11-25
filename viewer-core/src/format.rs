// SPDX-License-Identifier: GPL-3.0-only

use image::ImageFormat;
use std::{
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom},
    path::Path,
};

pub const EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jfif", "png", "apng", "gif", "webp", "bmp", "dib", "tiff", "tif", "ico",
    "avif", "avifs", "heif", "heic", "jxl", "svg", "jp2", "j2k", "j2c", "jpc", "qoi", "tga", "dds",
    "exr", "hdr", "rgbe", "pnm", "pbm", "pgm", "ppm", "pam", "ff", "farbfeld", "3fr", "arw", "cr2",
    "cr3", "crw", "dcr", "dng", "erf", "fff", "iiq", "kdc", "mef", "mos", "mrw", "nef", "nrw",
    "orf", "pef", "raf", "raw", "rw2", "rwl", "sr2", "srf", "srw", "x3f",
];

const RAW_EXTENSIONS: &[&str] = &[
    "3fr", "arw", "cr2", "cr3", "crw", "dcr", "dng", "erf", "fff", "iiq", "kdc", "mef", "mos",
    "mrw", "nef", "nrw", "orf", "pef", "raf", "raw", "rw2", "rwl", "sr2", "srf", "srw", "x3f",
];

pub const OUTPUT_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "avif", "ico", "tga", "qoi", "ff",
    "farbfeld", "pnm", "pbm", "pgm", "ppm", "pam", "hdr", "exr",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Raster(ImageFormat),
    Avif,
    Dib,
    Heif,
    Jpeg2000,
    Jpeg2000Codestream,
    JpegXl,
    Raw,
    Svg,
}

impl Format {
    pub fn mime(self) -> &'static str {
        match self {
            Self::Raster(ImageFormat::Dds) => "image/vnd.ms-dds",
            Self::Raster(ImageFormat::Qoi) => "image/qoi",
            Self::Raster(ImageFormat::Ico) => "image/vnd.microsoft.icon",
            Self::Raster(format) => format.to_mime_type(),
            Self::Avif => "image/avif",
            Self::Dib => "image/bmp",
            Self::Heif => "image/heif",
            Self::Jpeg2000 => "image/jp2",
            Self::Jpeg2000Codestream => "image/x-jp2-codestream",
            Self::JpegXl => "image/jxl",
            Self::Raw => "application/octet-stream",
            Self::Svg => "image/svg+xml",
        }
    }
}

pub fn output_format(path: &Path) -> Option<ImageFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !OUTPUT_EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }
    let ext = if ext == "farbfeld" { "ff" } else { &ext };
    ImageFormat::from_extension(ext).filter(ImageFormat::writing_enabled)
}

fn raw_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    RAW_EXTENSIONS
        .iter()
        .any(|candidate| ext.eq_ignore_ascii_case(candidate))
}

pub fn detect(path: &Path) -> io::Result<Option<Format>> {
    if !path.metadata()?.is_file() {
        return Ok(None);
    }
    let mut file = BufReader::new(File::open(path)?);
    let mut header = [0; 8192];
    let len = file.read(&mut header)?;
    let data = &header[..len];

    if data.get(4..8) == Some(b"ftyp") {
        let size = u32::from_be_bytes(header[..4].try_into().expect("four bytes")) as usize;
        let mut avif = false;
        let mut heif = false;
        let brands = data[8..size.min(len).max(8)].chunks_exact(4);
        let minor_version_index = 1;
        for (index, brand) in brands.enumerate() {
            if index == minor_version_index {
                continue;
            }
            match brand {
                b"crx " => return Ok(Some(Format::Raw)),
                b"avif" | b"avis" => avif = true,
                b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1" | b"heim" | b"heis" => {
                    heif = true;
                }
                _ => {}
            }
        }
        if avif || heif {
            return Ok(Some(if avif { Format::Avif } else { Format::Heif }));
        }
    }
    let special = if data.starts_with(&[0, 0, 0, 12, b'j', b'P', b' ', b' ', 13, 10, 0x87, 10]) {
        Some(Format::Jpeg2000)
    } else if data.starts_with(&[0xff, 0x4f, 0xff, 0x51]) {
        Some(Format::Jpeg2000Codestream)
    } else if data.starts_with(&[0xff, 0x0a])
        || data.starts_with(&[0, 0, 0, 12, b'J', b'X', b'L', b' ', 13, 10, 0x87, 10])
    {
        Some(Format::JpegXl)
    } else if [
        b"FUJIFILMCCD-RAW".as_slice(),
        b"FOVb",
        b"IIRO",
        b"IIRS",
        b"MMOR",
        b"IIU\0",
        b"\0MRM",
        b"MMMMRawT",
    ]
    .iter()
    .any(|magic| data.starts_with(magic))
        || data.get(8..12) == Some(b"CR\x02\0")
        || data.get(6..14) == Some(b"HEAPCCDR")
    {
        Some(Format::Raw)
    } else {
        None
    };
    if special.is_some() {
        return Ok(special);
    }

    let tiff = data.starts_with(b"II*\0")
        || data.starts_with(b"MM\0*")
        || data.starts_with(b"II+\0")
        || data.starts_with(b"MM\0+");
    if tiff {
        return Ok(Some(if raw_extension(path) || tiff_has_raw_tags(data) {
            Format::Raw
        } else {
            Format::Raster(ImageFormat::Tiff)
        }));
    }
    if let Ok(format) = image::guess_format(data) {
        if format != ImageFormat::Pnm || data.get(2).is_some_and(u8::is_ascii_whitespace) {
            return Ok(Some(Format::Raster(format)));
        }
    }
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("dib"))
        && data.len() >= 12
        && matches!(
            u32::from_le_bytes(header[..4].try_into().expect("DIB header")),
            12 | 40 | 52 | 56 | 64 | 108 | 124
        )
    {
        return Ok(Some(Format::Dib));
    }
    if data.starts_with(b"#?RGBE") || data.starts_with(b"#?RADIANCE") {
        return Ok(Some(Format::Raster(ImageFormat::Hdr)));
    }

    let tga = if len >= 26 {
        let mut footer = [0; 18];
        file.seek(SeekFrom::End(-18))?;
        file.read_exact(&mut footer)?;
        &footer == b"TRUEVISION-XFILE.\0"
    } else {
        false
    };
    if tga
        || (path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tga"))
            && len >= 18
            && matches!(data[2], 1 | 2 | 3 | 9 | 10 | 11)
            && data[12..14] != [0, 0]
            && data[14..16] != [0, 0])
    {
        return Ok(Some(Format::Raster(ImageFormat::Tga)));
    }

    let xml = std::str::from_utf8(data).unwrap_or_else(|error| {
        std::str::from_utf8(&data[..error.valid_up_to()]).unwrap_or_default()
    });
    if looks_like_svg(xml) {
        return Ok(Some(Format::Svg));
    }
    Ok(None)
}

fn looks_like_svg(mut xml: &str) -> bool {
    xml = xml.trim_start_matches('\u{feff}').trim_start();
    loop {
        let end = if xml.starts_with("<?") {
            "?>"
        } else if xml.starts_with("<!--") {
            "-->"
        } else if xml.starts_with("<!DOCTYPE svg") {
            ">"
        } else {
            break;
        };
        let Some((_, rest)) = xml.split_once(end) else {
            return false;
        };
        xml = rest.trim_start();
    }
    xml.strip_prefix("<svg")
        .is_some_and(|tail| tail.starts_with(['>', '/', ' ', '\t', '\n', '\r']))
}

fn tiff_has_raw_tags(data: &[u8]) -> bool {
    let little = data.starts_with(b"II");
    let word = |b: [u8; 2]| {
        if little {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        }
    };
    let long = |b: [u8; 4]| {
        if little {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        }
    };
    let Some(offset) = data.get(4..8).and_then(|b| b.try_into().ok()).map(long) else {
        return false;
    };
    let Some(ifd) = data.get(offset as usize..) else {
        return false;
    };
    let Some(count) = ifd.get(..2).and_then(|b| b.try_into().ok()).map(word) else {
        return false;
    };
    ifd[2..]
        .chunks_exact(12)
        .take(count.into())
        .any(|tag| matches!(word([tag[0], tag[1]]), 50706 | 33421 | 33422))
}
