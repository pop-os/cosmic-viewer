// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    ToolOperation,
    annotate::tool::text::{
        LINE_HEIGHT_FACTOR, TEXT_INSET, TextSpan, build_buffer_line, color_channel_u8, group_spans,
        intern_str,
        preview::{TextEditState, TextPreview},
        rotated_footprint, span_attrs,
    },
    rotate::RotateDirection,
};
use cosmic::{
    Renderer,
    iced::advanced::graphics::text::{cosmic_text, font_system},
    iced::advanced::text::{LineHeight, Shaping},
    iced::widget::canvas::{self, Frame},
    iced::{
        Color, Font, Point, Radians, Rectangle, Size, Vector,
        alignment::{Horizontal, Vertical},
        font,
    },
};
use image::{DynamicImage, Rgba, RgbaImage, imageops};
use std::any::Any;
use std::f32::consts::FRAC_PI_2;

#[derive(Debug, Clone)]
pub struct TextOperation {
    pub position: Point,
    pub spans: Vec<TextSpan>,
    pub color: Color,
    pub font_size: f32,
    pub font_family: &'static str,
    pub alignment: Horizontal,
    pub bounding_box: Rectangle,
    /// Number of 90° clockwise rotations applied (mod 4). The `bounding_box` is the
    /// axis-aligned image-space footprint; the text is laid out in its reading orientation
    /// (see `reading_size`) and rotated by this many quarter-turns at render/save time.
    pub rotation_steps: u8,
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
    /// Box dimensions in the text's reading (un-rotated) orientation. For odd quarter-turns
    /// the image-space `bounding_box` has width/height swapped, so swap them back.
    #[must_use]
    pub const fn reading_size(&self) -> Size {
        if self.rotation_steps % 2 == 1 {
            Size::new(self.bounding_box.height, self.bounding_box.width)
        } else {
            Size::new(self.bounding_box.width, self.bounding_box.height)
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
        // Edit in the upright reading orientation; rotation is re-applied on commit and rendered
        // live by the preview. The footprint->reading conversion is the same swap as reading->footprint.
        preview.bounding_box = rotated_footprint(self.bounding_box, self.rotation_steps);
        preview.rotation_steps = self.rotation_steps;
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
    fn build_buffer(&self) -> cosmic_text::Buffer {
        let metrics =
            cosmic_text::Metrics::new(self.font_size, self.font_size * LINE_HEIGHT_FACTOR);
        let default_attrs =
            cosmic_text::Attrs::new().family(cosmic_text::Family::Name(self.font_family));

        let mut font_sys = font_system().write().expect("Write font system");
        let mut buffer = cosmic_text::Buffer::new(font_sys.raw(), metrics);
        buffer.set_size(
            Some(TEXT_INSET.mul_add(-2.0, self.reading_size().width)),
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
                attrs_list.add_span(offset..end, &span_attrs(default_attrs.clone(), span));
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
    fn layout_runs(&self) -> Vec<SpanRun> {
        let buffer = self.build_buffer();
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
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, _scale: f32) {
        if self.spans.is_empty() {
            return;
        }

        let runs = self.layout_runs();
        let reading = self.reading_size();
        let center = self.bounding_box.center();
        // The text is laid out in its reading orientation, centered on the footprint, then the
        // frame is rotated about that center. For `rotation_steps == 0` this is exactly
        // `(bounding_box.x + TEXT_INSET, bounding_box.y)`.
        let origin = Point::new(
            center.x - reading.width / 2.0 + TEXT_INSET,
            center.y - reading.height / 2.0,
        );

        let steps = self.rotation_steps % 4;
        if steps != 0 {
            frame.push_transform();
            frame.translate(Vector::new(center.x, center.y));
            frame.rotate(Radians(f32::from(steps) * FRAC_PI_2));
            frame.translate(Vector::new(-center.x, -center.y));
        }

        for run in &runs {
            let text = canvas::Text {
                content: run.content.clone(),
                position: Point::new(
                    run.x + origin.x,
                    run.line_top
                        + run.font_size.mul_add(-LINE_HEIGHT_FACTOR, run.line_h)
                        + origin.y,
                ),
                color: run.color,
                size: run.font_size.into(),
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
                let uy = run.line_top + run.line_h + origin.y - 1.0;
                let ux = run.x + origin.x;
                let uw = run.x_end - run.x;
                frame.stroke(
                    &canvas::Path::line(Point::new(ux, uy), Point::new(ux + uw, uy)),
                    canvas::Stroke::default()
                        .with_color(run.color)
                        .with_width(1.0),
                );
            }
        }

        if steps != 0 {
            frame.pop_transform();
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

        let reading = self.reading_size();
        let img_font = self.font_size;
        let img_metrics = cosmic_text::Metrics::new(img_font, img_font * LINE_HEIGHT_FACTOR);
        let default_attrs =
            cosmic_text::Attrs::new().family(cosmic_text::Family::Name(self.font_family));

        let mut font_sys = font_system().write().expect("Write font system");
        let mut buffer = cosmic_text::Buffer::new(font_sys.raw(), img_metrics);
        buffer.set_size(Some(TEXT_INSET.mul_add(-2.0, reading.width)), None);
        buffer.set_wrap(cosmic_text::Wrap::WordOrGlyph);

        let lines = group_spans(&self.spans);
        buffer.lines.clear();
        for (line_text, line_spans, line_align) in &lines {
            let mut attrs_list = cosmic_text::AttrsList::new(&default_attrs);
            let mut offset = 0;
            for span in line_spans {
                let end = offset + span.text.len();
                attrs_list.add_span(offset..end, &span_attrs(default_attrs.clone(), span));
                offset = end;
            }
            buffer
                .lines
                .push(build_buffer_line(line_text, attrs_list, *line_align));
        }
        buffer.shape_until_scroll(font_sys.raw(), false);

        // Rasterize the text upright into a transparent layer the size of the reading box, then
        // rotate that layer into the image's orientation. 90° turns stay exact and avoid
        // per-glyph bitmap rotation.
        let layer_w = (reading.width.ceil() as u32).max(1);
        let layer_h = (reading.height.ceil() as u32).max(1);
        let mut layer = RgbaImage::new(layer_w, layer_h);
        let layer_bounds = (layer_w as i32, layer_h as i32);

        let fallback_color = cosmic_text::Color::rgba(
            color_channel_u8(self.color.r),
            color_channel_u8(self.color.g),
            color_channel_u8(self.color.b),
            color_channel_u8(self.color.a),
        );

        let mut swash_cache = cosmic_text::SwashCache::new();
        let origin = Point::new(TEXT_INSET, 0.0);
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

                        if px >= 0 && px < layer_bounds.0 && py >= 0 && py < layer_bounds.1 {
                            let alpha = f32::from(color.a()) / 255.0;
                            if alpha > 0.0 {
                                let dst = *layer.get_pixel(px as u32, py as u32);
                                layer.put_pixel(
                                    px as u32,
                                    py as u32,
                                    blend_over(dst, color.r(), color.g(), color.b(), alpha),
                                );
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
                &mut layer,
                layer_bounds,
            );
        }
        drop(font_sys);

        // Rotate the upright layer into the image orientation (clockwise quarter-turns).
        let layer = match self.rotation_steps % 4 {
            1 => imageops::rotate90(&layer),
            2 => imageops::rotate180(&layer),
            3 => imageops::rotate270(&layer),
            _ => layer,
        };

        // Composite the (rotated) layer onto the image at the footprint's top-left.
        let rgba = image.as_mut_rgba8().expect("image should be RGBA");
        composite_over(
            rgba,
            &layer,
            self.bounding_box.x.round() as i32,
            self.bounding_box.y.round() as i32,
        );
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
        let (w, h) = (image_size.width, image_size.height);
        let b = self.bounding_box;
        // Map the axis-aligned footprint into the rotated image; a 90° turn swaps width/height.
        // (`image_size` is the pre-rotation image size, matching the pixel rotate90/rotate270.)
        self.bounding_box = match direction {
            RotateDirection::Right => Rectangle::new(
                Point::new(h - b.y - b.height, b.x),
                Size::new(b.height, b.width),
            ),
            RotateDirection::Left => Rectangle::new(
                Point::new(b.y, w - b.x - b.width),
                Size::new(b.height, b.width),
            ),
        };
        self.position = self.bounding_box.position();
        self.rotation_steps = match direction {
            RotateDirection::Right => (self.rotation_steps + 1) % 4,
            RotateDirection::Left => (self.rotation_steps + 3) % 4,
        };
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

/// Source-over composite of `src` onto `dst` with its top-left at `(ox, oy)` in `dst` space.
// reason: layer/image pixel coordinates are within i32/u32 range; truncation is intended, bounds checked.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
fn composite_over(dst: &mut RgbaImage, src: &RgbaImage, ox: i32, oy: i32) {
    let (dw, dh) = (dst.width() as i32, dst.height() as i32);
    for (lx, ly, px) in src.enumerate_pixels() {
        let alpha = f32::from(px[3]) / 255.0;
        if alpha <= 0.0 {
            continue;
        }
        let dx = ox + lx as i32;
        let dy = oy + ly as i32;
        if dx >= 0 && dx < dw && dy >= 0 && dy < dh {
            let under = *dst.get_pixel(dx as u32, dy as u32);
            dst.put_pixel(
                dx as u32,
                dy as u32,
                blend_over(under, px[0], px[1], px[2], alpha),
            );
        }
    }
}

/// Straight-alpha source-over of an `(sr, sg, sb)` color with coverage `sa` (0.0..=1.0) onto `dst`.
/// Reduces to a simple lerp when `dst` is opaque, but stays correct for a transparent destination
/// (the rasterization layer), where a naive `src*a + dst*(1-a)` would premultiply the color twice.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // reason: clamped to 0.0..=255.0 then rounded, in-range for u8
fn blend_over(dst: Rgba<u8>, sr: u8, sg: u8, sb: u8, sa: f32) -> Rgba<u8> {
    let da = f32::from(dst[3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return Rgba([0, 0, 0, 0]);
    }
    let chan = |s: u8, d: u8| -> u8 {
        (f32::from(s).mul_add(sa, f32::from(d) * da * (1.0 - sa)) / out_a)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgba([
        chan(sr, dst[0]),
        chan(sg, dst[1]),
        chan(sb, dst[2]),
        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    ])
}
