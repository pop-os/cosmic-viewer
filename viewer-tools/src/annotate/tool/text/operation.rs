use crate::{
    ToolOperation,
    annotate::tool::text::{TextSpan, measure_span_width},
    rotate::RotateDirection,
};
use cosmic::{
    Renderer,
    iced::{
        Color, Font, Point, Rectangle, Size,
        alignment::{Horizontal, Vertical},
        font,
    },
    iced_core::text::{LineHeight, Shaping},
    iced_widget::{
        canvas::{self, Frame},
        graphics::text::{cosmic_text, font_system},
    },
};
use image::{DynamicImage, Rgba};
use std::any::Any;

#[derive(Debug, Clone)]
pub struct TextOperation {
    pub position: Point,
    pub spans: Vec<TextSpan>,
    pub color: Color,
    pub font_size: f32,
    pub font_family: &'static str,
    pub alignment: Horizontal,
}

impl TextOperation {
    pub fn new(
        position: Point,
        spans: Vec<TextSpan>,
        color: Color,
        font_size: f32,
        font_family: &'static str,
        alignment: Horizontal,
    ) -> Self {
        Self {
            position,
            spans,
            color,
            font_size,
            font_family,
            alignment,
        }
    }
}

impl ToolOperation for TextOperation {
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        if self.spans.is_empty() {
            return;
        }

        let mut x_offset = 0.0;
        for span in &self.spans {
            if span.text.is_empty() {
                continue;
            }

            let text = canvas::Text {
                content: span.text.clone(),
                position: Point::new(self.position.x + x_offset, self.position.y),
                color: self.color,
                size: (self.font_size / scale).into(),
                font: Font {
                    family: font::Family::Name(self.font_family),
                    weight: if span.bold {
                        font::Weight::Bold
                    } else {
                        font::Weight::Normal
                    },
                    style: if span.italic {
                        font::Style::Italic
                    } else {
                        font::Style::Normal
                    },
                    stretch: font::Stretch::Normal,
                },
                line_height: LineHeight::default(),
                horizontal_alignment: self.alignment,
                vertical_alignment: Vertical::Top,
                shaping: Shaping::Advanced,
            };
            frame.fill_text(text);

            x_offset += measure_span_width(
                &span.text,
                self.font_size,
                self.font_family,
                span.bold,
                span.italic,
            ) / scale;
        }
    }

    fn apply(&self, image: &mut DynamicImage) {
        if self.spans.is_empty() {
            return;
        }

        let full_text: String = self.spans.iter().map(|span| span.text.as_str()).collect();

        if full_text.trim().is_empty() {
            return;
        }

        let rgba = image.as_mut_rgba8().expect("image should be RGBA");

        let base_color = cosmic_text::Color::rgba(
            (self.color.r * 255.0) as u8,
            (self.color.g * 255.0) as u8,
            (self.color.b * 255.0) as u8,
            (self.color.a * 255.0) as u8,
        );

        let mut font_sys = font_system().write().expect("Write font system");
        let default_attrs =
            cosmic_text::Attrs::new().family(cosmic_text::Family::Name(self.font_family));
        let mut attrs_list = cosmic_text::AttrsList::new(&default_attrs);
        let mut byte_offset = 0;

        for span in &self.spans {
            let end = byte_offset + span.text.len();
            let span_attrs = default_attrs
                .clone()
                .weight(if span.bold {
                    cosmic_text::Weight::BOLD
                } else {
                    cosmic_text::Weight::NORMAL
                })
                .style(if span.italic {
                    cosmic_text::Style::Italic
                } else {
                    cosmic_text::Style::Normal
                });
            attrs_list.add_span(byte_offset..end, &span_attrs);
            byte_offset = end;
        }

        let mut buffer_line = cosmic_text::BufferLine::new(
            &full_text,
            cosmic_text::LineEnding::default(),
            attrs_list,
            cosmic_text::Shaping::Advanced,
        );

        let layout = buffer_line.layout(
            font_sys.raw(),
            self.font_size,
            None,
            cosmic_text::Wrap::None,
            cosmic_text::Ellipsize::None,
            None,
            8,
            cosmic_text::Hinting::Disabled,
        );

        let mut swash_cache = cosmic_text::SwashCache::new();
        let (img_width, img_height) = (rgba.width() as i32, rgba.height() as i32);

        for run in layout.iter() {
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, 0.0), 1.0);
                let glyph_x = self.position.x + glyph.x + glyph.x_offset;
                let glyph_y = self.position.y + glyph.y_offset + self.font_size;

                swash_cache.with_pixels(
                    font_sys.raw(),
                    physical.cache_key,
                    base_color,
                    |off_x, off_y, color| {
                        let pos_x = (glyph_x as i32) + off_x;
                        let pos_y = (glyph_y as i32) + off_y;

                        if pos_x >= 0 && pos_x < img_width && pos_y >= 0 && pos_y < img_height {
                            let alpha = color.a() as f32 / 255.0;
                            if alpha > 0.0 {
                                let existing = rgba.get_pixel(pos_x as u32, pos_y as u32);
                                let blended = Rgba([
                                    blend_channel(existing[0], color.r(), alpha),
                                    blend_channel(existing[1], color.g(), alpha),
                                    blend_channel(existing[2], color.b(), alpha),
                                    (existing[3] as f32 * (1.0 - alpha) + 255.0 * alpha).min(255.0)
                                        as u8,
                                ]);
                                rgba.put_pixel(pos_x as u32, pos_y as u32, blended);
                            }
                        }
                    },
                );
            }
        }

        *image = DynamicImage::ImageRgba8(rgba.clone());
    }

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn transform_rotate(&mut self, direction: RotateDirection, image_size: Size) {
        let (width, height) = (image_size.width, image_size.height);
        let (x, y) = (self.position.x, self.position.y);

        self.position = match direction {
            RotateDirection::Left => Point::new(y, width - x),
            RotateDirection::Right => Point::new(height - y, x),
        }
    }

    fn transform_crop(&mut self, region: Rectangle) {
        self.position.x -= region.x;
        self.position.y -= region.y;
    }
}

fn blend_channel(dst: u8, src: u8, alpha: f32) -> u8 {
    ((dst as f32) * (1.0 - alpha) + (src as f32) * alpha).min(255.0) as u8
}
