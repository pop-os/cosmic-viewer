use std::borrow::Cow;

use cosmic::iced::clipboard::mime::AsMimeTypes;

#[derive(Debug, Clone)]
pub struct ClipboardImage {
    pub data: Vec<u8>,
    pub mime: String,
}

impl AsMimeTypes for ClipboardImage {
    fn available(&self) -> Cow<'static, [String]> {
        Cow::Owned(vec![self.mime.clone()])
    }

    fn as_bytes(&self, mime_type: &str) -> Option<Cow<'static, [u8]>> {
        if mime_type == self.mime {
            Some(Cow::Owned(self.data.clone()))
        } else {
            None
        }
    }
}

pub fn image_mime_type(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "tiff" | "tif" => Some("image/tiff"),
        "avif" => Some("image/avif"),
        "ico" => Some("image/x-ico"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}
