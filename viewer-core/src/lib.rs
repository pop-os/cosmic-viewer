// SPDX-License-Identifier: GPL-3.0-only

mod animation;
mod apng;
mod color;
mod export;
mod format;
mod jxl;
mod native;
mod pages;

pub mod cache;
pub mod clipboard;
pub mod loader;
pub mod nav;

pub use animation::{Animation, AnimationFrame, OpenedImage, open_image};
pub use export::save_image;
pub use format::{EXTENSIONS, Format, OUTPUT_EXTENSIONS, detect, output_format};
pub use pages::load_page;

// Re-exports
pub use cache::{CachedImage, ImageCache};
pub use clipboard::{ClipboardImage, image_mime_type};
pub use loader::{LoadError, LoadedImage, load_image, load_thumbnail, read_dpi};
pub use nav::{NavState, get_image_dir, is_supported_image, scan_dir};
