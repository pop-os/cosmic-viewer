use crate::{
    ToolOperation,
    annotate::tool::text::{
        LINE_HEIGHT_FACTOR, TEXT_INSET, TextSpan, build_buffer_line, color_channel_u8, group_spans,
        intern_str, span_attrs,
        preview::{TextEditState, TextPreview},
    },
    rotate::RotateDirection,
};
use cosmic::{
    Renderer,
    iced::{
        Color, Font, Point, Rectangle, Size,
        alignment::{Horizontal, Vertical},
        font,
    },
    iced::advanced::text::{LineHeight, Shaping},
    iced::widget::canvas::{self, Frame},
    iced::advanced::graphics::text::{cosmic_text, font_system},
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
    pub bounding_box: Rectangle,
    pub edit_scale: f32,
}

/// A run of identically-styled glyphs, laid out for canvas rendering.
struct SpanRun {
    content: String,
    x: f32,
    x_end: f32,
    line_top: f32,
    line_h: f32,
    bold: bool,
    italic: bool,
    underline: bool,
    color: Color,
    font_size: f32,
    family: &'static str,
}

impl TextOperation {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        position: Point,
        spans: Vec<TextSpan>,
        color: Color,
        font_size: f32,
        font_family: &'static str,
        alignment: Horizontal,
        bounding_box: Rectangle,
        edit_scale: f32,
    ) -> Self {
        Self {
            position,
            spans,
            color,
            font_size,
            font_family,
            alignment,
            bounding_box,
            edit_scale,
        }
    }
}

impl TextOperation {
    #[must_use]
    pub const fn bounds(&self) -> Rectangle {
        self.bounding_box
    }

    #[must_use]
    pub fn hit_test(&self, point: Point) -> bool {
        self.bounds().contains(point)
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.position.x += dx;
        self.position.y += dy;
        self.bounding_box.x += dx;
        self.bounding_box.y += dy;
    }

    #[must_use]
    pub fn to_preview(&self) -> TextPreview {
        let mut preview = TextPreview::new(
            self.color,
            self.font_size,
            self.font_family,
            false,
            false,
            false,
            self.alignment,
        );
        preview.bounding_box = self.bounding_box;
        preview.last_scale.set(self.edit_scale);
        preview.state = TextEditState::Editing;

        if let Some(last) = self.spans.last() {
            preview.bold = last.bold;
            preview.italic = last.italic;
            preview.underline = last.underline;
            if let Some([r, g, b, a]) = last.color {
                preview.color = cosmic::iced::Color::from_rgba(r, g, b, a);
            }
            if let Some(fs) = last.font_size {
                preview.font_size = fs;
            }
        }

        preview.init_editor_from_spans(&self.spans);
        preview
    }
}

impl TextOperation {
    fn build_buffer(&self, scale: f32) -> cosmic_text::Buffer {
        let metrics =
            cosmic_text::Metrics::new(self.font_size, self.font_size * LINE_HEIGHT_FACTOR);
        let default_attrs =
            cosmic_text::Attrs::new().family(cosmic_text::Family::Name(self.font_family));

        let mut font_sys = font_system().write().expect("Write font system");
        let mut buffer = cosmic_text::Buffer::new(font_sys.raw(), metrics);
        buffer.set_size(
            Some(TEXT_INSET.mul_add(-2.0, self.bounding_box.width) * scale),
            None,
        );
        buffer.set_wrap(cosmic_text::Wrap::WordOrGlyph);

        let lines = group_spans(&self.spans);
        buffer.lines.clear();
        for (line_text, line_spans, line_align) in &lines {
            let mut attrs_list = cosmic_text::AttrsList::new(&default_attrs);
            let mut offset = 0;
            for span in line_spans {
                let end = offset + span.text.len();
                attrs_list.add_span(offset..end, &span_attrs(default_attrs.clone(), span, 1.0));
                offset = end;
            }
            buffer
                .lines
                .push(build_buffer_line(line_text, attrs_list, *line_align));
        }
        buffer.shape_until_scroll(font_sys.raw(), false);
        buffer
    }

    /// Lay the text out and collect runs of identically-styled glyphs for rendering.
    fn layout_runs(&self, scale: f32) -> Vec<SpanRun> {
        let buffer = self.build_buffer(scale);
        let mut runs = Vec::new();
        for run in buffer.layout_runs() {
            if run.glyphs.is_empty() {
                continue;
            }
            let line = &buffer.lines[run.line_i];
            let attrs_list = line.attrs_list();
            let mut start = 0;
            while start < run.glyphs.len() {
                let first = &run.glyphs[start];
                let attrs = attrs_list.get_span(first.start);
                let mut end = start + 1;
                while end < run.glyphs.len() {
                    if attrs_list.get_span(run.glyphs[end].start) != attrs {
                        break;
                    }
                    end += 1;
                }
                let last = &run.glyphs[end - 1];
                let color = attrs.color_opt.map_or(self.color, |c| {
                    Color::from_rgba(
                        f32::from(c.r()) / 255.0,
                        f32::from(c.g()) / 255.0,
                        f32::from(c.b()) / 255.0,
                        f32::from(c.a()) / 255.0,
                    )
                });
                let font_size = attrs.metrics_opt.map_or(self.font_size, |m| {
                    let metrics: cosmic_text::Metrics = m.into();
                    metrics.font_size
                });
                let family: &'static str = match attrs.family {
                    cosmic_text::Family::Name(n) if n != self.font_family => intern_str(n),
                    _ => self.font_family,
                };
                runs.push(SpanRun {
                    content: run.text[first.start..last.end].to_string(),
                    x: first.x,
                    x_end: last.x + last.w,
                    line_top: run.line_top,
                    line_h: run.line_height,
                    bold: attrs.weight >= cosmic_text::Weight::BOLD,
                    italic: attrs.style == cosmic_text::Style::Italic,
                    underline: attrs.metadata == 1,
                    color,
                    font_size,
                    family,
                });
                start = end;
            }
        }
        runs
    }
}

impl ToolOperation for TextOperation {
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        if self.spans.is_empty() {
            return;
        }

        let runs = self.layout_runs(scale);

        let inv = 1.0 / scale;
        let inset = TEXT_INSET / scale;
        let origin = Point::new(self.bounding_box.x + inset, self.bounding_box.y);

        for run in &runs {
            let text = canvas::Text {
                content: run.content.clone(),
                position: Point::new(
                    run.x.mul_add(inv, origin.x),
                    (run.line_top + run.font_size.mul_add(-LINE_HEIGHT_FACTOR, run.line_h))
                        .mul_add(inv, origin.y),
                ),
                color: run.color,
                size: (run.font_size / scale).into(),
                font: Font {
                    family: font::Family::Name(run.family),
                    weight: if run.bold {
                        font::Weight::Bold
                    } else {
                        font::Weight::Normal
                    },
                    style: if run.italic {
                        font::Style::Italic
                    } else {
                        font::Style::Normal
                    },
                    stretch: font::Stretch::Normal,
                },
                max_width: f32::INFINITY,
                line_height: LineHeight::default(),
                align_x: Horizontal::Left.into(),
                align_y: Vertical::Top,
                shaping: Shaping::Advanced,
            };
            frame.fill_text(text);

            if run.underline {
                let uy = (run.line_top + run.line_h).mul_add(inv, origin.y) - 1.0 / scale;
                let ux = run.x.mul_add(inv, origin.x);
                let uw = (run.x_end - run.x) * inv;
                frame.stroke(
                    &canvas::Path::line(Point::new(ux, uy), Point::new(ux + uw, uy)),
                    canvas::Stroke::default()
                        .with_color(run.color)
                        .with_width(1.0 / scale),
                );
            }
        }
    }

    // reason: image dimensions and glyph offsets are pixel coordinates well within i32/u32 range;
    // float->int truncation is the intended quantization and bounds are explicitly checked before indexing.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    fn apply(&self, image: &mut DynamicImage) {
        if self.spans.is_empty() || self.bounding_box.width < 1.0 {
            return;
        }

        let img_font = self.font_size / self.edit_scale;
        let img_metrics = cosmic_text::Metrics::new(img_font, img_font * LINE_HEIGHT_FACTOR);
        let default_attrs =
            cosmic_text::Attrs::new().family(cosmic_text::Family::Name(self.font_family));
        let font_scale = 1.0 / self.edit_scale;

        let mut font_sys = font_system().write().expect("Write font system");
        let mut buffer = cosmic_text::Buffer::new(font_sys.raw(), img_metrics);
        buffer.set_size(Some(TEXT_INSET.mul_add(-2.0, self.bounding_box.width)), None);
        buffer.set_wrap(cosmic_text::Wrap::WordOrGlyph);

        let lines = group_spans(&self.spans);
        buffer.lines.clear();
        for (line_text, line_spans, line_align) in &lines {
            let mut attrs_list = cosmic_text::AttrsList::new(&default_attrs);
            let mut offset = 0;
            for span in line_spans {
                let end = offset + span.text.len();
                attrs_list.add_span(
                    offset..end,
                    &span_attrs(default_attrs.clone(), span, font_scale),
                );
                offset = end;
            }
            buffer
                .lines
                .push(build_buffer_line(line_text, attrs_list, *line_align));
        }
        buffer.shape_until_scroll(font_sys.raw(), false);

        let rgba = image.as_mut_rgba8().expect("image should be RGBA");
        let (img_w, img_h) = (rgba.width() as i32, rgba.height() as i32);

        let fallback_color = cosmic_text::Color::rgba(
            color_channel_u8(self.color.r),
            color_channel_u8(self.color.g),
            color_channel_u8(self.color.b),
            color_channel_u8(self.color.a),
        );

        let mut swash_cache = cosmic_text::SwashCache::new();
        let origin = Point::new(self.bounding_box.x + TEXT_INSET, self.bounding_box.y);
        for run in buffer.layout_runs() {
            let line = &buffer.lines[run.line_i];
            let attrs_list = line.attrs_list();

            for glyph in run.glyphs {
                let glyph_color = attrs_list
                    .get_span(glyph.start)
                    .color_opt
                    .unwrap_or(fallback_color);

                let physical = glyph.physical((0.0, 0.0), 1.0);
                let gx = origin.x + glyph.x + glyph.x_offset;
                let gy = origin.y + run.line_y + glyph.y_offset;

                swash_cache.with_pixels(
                    font_sys.raw(),
                    physical.cache_key,
                    glyph_color,
                    |off_x, off_y, color| {
                        let px = (gx as i32) + off_x;
                        let py = (gy as i32) + off_y;

                        if px >= 0 && px < img_w && py >= 0 && py < img_h {
                            let alpha = f32::from(color.a()) / 255.0;
                            if alpha > 0.0 {
                                let existing = rgba.get_pixel(px as u32, py as u32);
                                let out_alpha = 255.0f32
                                    .mul_add(alpha, f32::from(existing[3]) * (1.0 - alpha))
                                    .round()
                                    .clamp(0.0, 255.0) as u8;
                                let blended = Rgba([
                                    blend_channel(existing[0], color.r(), alpha),
                                    blend_channel(existing[1], color.g(), alpha),
                                    blend_channel(existing[2], color.b(), alpha),
                                    out_alpha,
                                ]);
                                rgba.put_pixel(px as u32, py as u32, blended);
                            }
                        }
                    },
                );
            }

            draw_run_underlines(
                &run,
                attrs_list,
                origin,
                img_font,
                fallback_color,
                rgba,
                (img_w, img_h),
            );
        }
        drop(font_sys);

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
        self.bounding_box.x -= region.x;
        self.bounding_box.y -= region.y;
    }

    fn movable(&self) -> bool {
        true
    }

    fn hit_test(&self, point: Point) -> bool {
        Self::hit_test(self, point)
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        Self::translate(self, dx, dy);
    }

    fn bounds(&self) -> Option<Rectangle> {
        Some(Self::bounds(self))
    }
}

// reason: underline pixel coordinates are within i32/u32 range; float->int truncation is intended and bounds are checked.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
fn draw_run_underlines(
    run: &cosmic_text::LayoutRun,
    attrs_list: &cosmic_text::AttrsList,
    origin: Point,
    img_font: f32,
    fallback_color: cosmic_text::Color,
    rgba: &mut image::RgbaImage,
    (img_w, img_h): (i32, i32),
) {
    let mut start = 0;
    while start < run.glyphs.len() {
        let first = &run.glyphs[start];
        let attrs = attrs_list.get_span(first.start);
        let mut end = start + 1;
        while end < run.glyphs.len() {
            if attrs_list.get_span(run.glyphs[end].start) != attrs {
                break;
            }
            end += 1;
        }

        if attrs.metadata == 1 {
            let last = &run.glyphs[end - 1];
            let span_fs = attrs.metrics_opt.map_or(img_font, |m| {
                let metrics: cosmic_text::Metrics = m.into();
                metrics.font_size
            });
            let uy = (span_fs.mul_add(LINE_HEIGHT_FACTOR, origin.y + run.line_top) + 2.0) as i32;
            let x_start = (origin.x + first.x) as i32;
            let x_end = (origin.x + last.x + last.w) as i32;

            let underline_color = attrs.color_opt.unwrap_or(fallback_color);
            let pixel = Rgba([
                underline_color.r(),
                underline_color.g(),
                underline_color.b(),
                underline_color.a(),
            ]);

            if uy >= 0 && uy < img_h {
                for x in x_start.max(0)..x_end.min(img_w) {
                    rgba.put_pixel(x as u32, uy as u32, pixel);
                }
            }
        }

        start = end;
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // reason: clamped to 0.0..=255.0 then rounded, in-range for u8
fn blend_channel(dst: u8, src: u8, alpha: f32) -> u8 {
    f32::from(src)
        .mul_add(alpha, f32::from(dst) * (1.0 - alpha))
        .round()
        .clamp(0.0, 255.0) as u8
}
