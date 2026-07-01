use super::{
    BORDER_WIDTH, DEFAULT_BOX_WIDTH, DRAG_THRESHOLD, LINE_HEIGHT_FACTOR, MIN_BOX_HEIGHT,
    MIN_BOX_WIDTH, TextDragHandle, TextOperation, TextSpan, build_buffer_line, color_channel_u8,
    content_height, encode_align, group_spans, intern_str, rotated_footprint, snap_font_size,
    span_attrs,
};
use crate::{ToolOperation, annotate::tool::text::TEXT_INSET};
use cosmic::{
    Renderer,
    iced::advanced::graphics::text::{cosmic_text, font_system},
    iced::advanced::text::{LineHeight, Shaping},
    iced::widget::canvas::{self, Fill, Frame, Path, Stroke},
    iced::{
        Color, Font, Point, Radians, Rectangle, Size, Vector,
        alignment::{Horizontal, Vertical},
        font, mouse,
    },
};
use cosmic_text::Edit;
use image::DynamicImage;
use std::{any::Any, cell::Cell, f32, time::Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextEditState {
    Placing,
    PlacingDrag,
    Editing,
    Resizing,
}

// reason: bold/italic/underline are independent rich-text attributes and the two
// interaction flags track orthogonal drag state; they are not a single enum state.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct TextPreview {
    pub color: Color,
    pub font_size: f32,
    pub font_family: &'static str,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub alignment: Horizontal,
    pub state: TextEditState,

    pub bounding_box: Rectangle,
    pub last_scale: Cell<f32>,
    editor: Option<cosmic_text::Editor<'static>>,
    active_handle: TextDragHandle,
    drag_origin: Point,
    drag_start_box: Rectangle,
    custom_dragged: bool,
    mouse_selecting: bool,
    blink_start: Cell<Instant>,
    last_click: Option<std::time::Instant>,
    click_count: u8,
    /// 90° clockwise quarter-turns this text carries (mod 4); set when re-editing an
    /// already-rotated committed text so commit can restore it. Editing is always upright.
    pub rotation_steps: u8,
}

impl TextPreview {
    #[must_use]
    pub fn new(
        color: Color,
        font_size: f32,
        font_family: &'static str,
        bold: bool,
        italic: bool,
        underline: bool,
        alignment: Horizontal,
    ) -> Self {
        Self {
            color,
            font_size,
            font_family,
            bold,
            italic,
            underline,
            alignment,
            state: TextEditState::Placing,
            bounding_box: Rectangle::new(Point::ORIGIN, Size::ZERO),
            last_scale: Cell::new(1.0),
            editor: None,
            active_handle: TextDragHandle::None,
            drag_origin: Point::ORIGIN,
            mouse_selecting: false,
            blink_start: Cell::new(Instant::now()),
            last_click: None,
            click_count: 0,
            drag_start_box: Rectangle::new(Point::ORIGIN, Size::ZERO),
            custom_dragged: false,
            rotation_steps: 0,
        }
    }

    /// Clockwise quarter-turns of the editing rotation (mod 4).
    const fn steps(&self) -> u8 {
        self.rotation_steps % 4
    }

    /// Inverse-rotate an image-space point into the upright (reading) box frame about `center`.
    /// Editing happens in the upright box, but the render is rotated, so input on the rotated
    /// text must be un-rotated to line up with the upright hit-testing/cursor logic.
    fn unrotate(&self, p: Point, center: Point) -> Point {
        let (dx, dy) = (p.x - center.x, p.y - center.y);
        // Inverse of the render rotation; 90° and 270° are each other's inverse (180° is
        // self-inverse, which is why only the odd turns were mis-mapped).
        let (rx, ry) = match self.steps() {
            1 => (dy, -dx),
            2 => (-dx, -dy),
            3 => (-dy, dx),
            _ => (dx, dy),
        };
        Point::new(center.x + rx, center.y + ry)
    }

    /// Inverse-rotate about the current box center, for press/hover hit-testing.
    fn to_local(&self, p: Point) -> Point {
        self.unrotate(p, self.bounding_box.center())
    }

    /// Which handle (if any) a raw image-space point hits, accounting for the editing rotation.
    /// Callers outside the preview (e.g. the app's press router) must use this rather than
    /// `hit_test_handle` directly, which expects an already-un-rotated point.
    #[must_use]
    pub fn handle_at(&self, image_point: Point) -> TextDragHandle {
        self.hit_test_handle(self.to_local(image_point))
    }

    fn init_editor(&mut self) {
        let metrics =
            cosmic_text::Metrics::new(self.font_size, self.font_size * LINE_HEIGHT_FACTOR);
        let mut buffer = {
            let mut font_sys = font_system().write().expect("Write font system");
            cosmic_text::Buffer::new(font_sys.raw(), metrics)
        };
        buffer.set_size(Some(TEXT_INSET.mul_add(-2.0, self.bounding_box.width)), None);
        buffer.set_wrap(cosmic_text::Wrap::WordOrGlyph);
        self.editor = Some(cosmic_text::Editor::new(buffer));
    }

    pub fn update_font_size(&mut self, size: f32) {
        self.font_size = size;
        // Per-character: font size is applied via current_attrs() on insert
        // If there's a selection, apply to the selected range
        if self.has_selection() {
            self.apply_attr_to_selection(|a| {
                a.metrics(cosmic_text::Metrics::new(size, size * LINE_HEIGHT_FACTOR))
            });
        }
    }

    /// # Panics
    ///
    /// Panics if the shared font-system lock is poisoned.
    pub fn init_editor_with_text(&mut self, text: &str) {
        self.init_editor();
        let attrs = self.current_attrs();
        if let Some(editor) = &mut self.editor {
            let mut font_sys = font_system().write().expect("Write font system");
            editor.with_buffer_mut(|buf| {
                buf.set_text(text, &attrs, cosmic_text::Shaping::Advanced, None);
            });
            editor.shape_as_needed(font_sys.raw(), false);
            drop(font_sys);

            let content_h: f32 = editor.with_buffer(content_height);
            if content_h > self.bounding_box.height {
                self.bounding_box.height = content_h;
            }
        }
    }

    /// # Panics
    ///
    /// Panics if the shared font-system lock is poisoned.
    pub fn init_editor_from_spans(&mut self, spans: &[TextSpan]) {
        self.init_editor();
        if let Some(editor) = &mut self.editor {
            let default_attrs =
                cosmic_text::Attrs::new().family(cosmic_text::Family::Name(self.font_family));
            let lines = group_spans(spans);

            let mut font_sys = font_system().write().expect("Write font system");
            editor.with_buffer_mut(|buf| {
                buf.lines.clear();
                for (line_text, line_spans, line_align) in &lines {
                    let mut attrs_list = cosmic_text::AttrsList::new(&default_attrs);
                    let mut offset = 0;
                    for span in line_spans {
                        let end = offset + span.text.len();
                        attrs_list.add_span(offset..end, &span_attrs(default_attrs.clone(), span));
                        offset = end;
                    }
                    buf.lines
                        .push(build_buffer_line(line_text, attrs_list, *line_align));
                }
                buf.shape_until_scroll(font_sys.raw(), false);
            });

            let content_h = editor.with_buffer(content_height);
            if content_h > self.bounding_box.height {
                self.bounding_box.height = content_h;
            }
        }
    }

    pub fn has_selection(&self) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|ed| ed.selection_bounds().is_some())
    }

    /// # Panics
    ///
    /// Panics if the shared font-system lock is poisoned.
    pub fn apply_attr_to_selection<F>(&mut self, modify: F)
    where
        F: Fn(cosmic_text::Attrs) -> cosmic_text::Attrs,
    {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let Some((start, end)) = editor.selection_bounds() else {
            return;
        };

        let mut font_sys = font_system().write().expect("Write font system");
        editor.with_buffer_mut(|buf| {
            for line_i in start.line..=end.line.min(buf.lines.len() - 1) {
                let line = &mut buf.lines[line_i];
                let text_len = line.text().len();
                let from = if line_i == start.line { start.index } else { 0 };
                let to = if line_i == end.line {
                    end.index
                } else {
                    text_len
                };
                let to = to.min(text_len);

                if from >= to || text_len == 0 {
                    continue;
                }

                let old_attrs = line.attrs_list().clone();
                let mut new_attrs = old_attrs.clone();

                // Apply the modification to each byte in the range
                for i in from..to {
                    let a = old_attrs.get_span(i);
                    new_attrs.add_span(i..i + 1, &modify(a));
                }

                line.set_attrs_list(new_attrs);
            }
            buf.shape_until_scroll(font_sys.raw(), false);
        });
    }

    pub fn motion_with_shift(&mut self, motion: cosmic_text::Motion, shift: bool) {
        if let Some(editor) = &mut self.editor {
            if shift {
                if editor.selection() == cosmic_text::Selection::None {
                    editor.set_selection(cosmic_text::Selection::Normal(editor.cursor()));
                }
            } else if editor.selection() != cosmic_text::Selection::None {
                editor.set_selection(cosmic_text::Selection::None);
            }
        }
        self.editor_action(cosmic_text::Action::Motion(motion));
    }

    pub fn select_all(&mut self) {
        if let Some(editor) = &mut self.editor {
            editor.set_cursor(cosmic_text::Cursor::new(0, 0));
            let last_line = editor.with_buffer(|buf| buf.lines.len().saturating_sub(1));
            let last_idx = editor.with_buffer(|buf| buf.lines.last().map_or(0, |l| l.text().len()));
            editor.set_selection(cosmic_text::Selection::Normal(cosmic_text::Cursor::new(
                0, 0,
            )));
            editor.set_cursor(cosmic_text::Cursor::new(last_line, last_idx));
        }
    }

    #[must_use]
    pub fn copy_selection(&self) -> Option<String> {
        self.editor.as_ref().and_then(Edit::copy_selection)
    }

    /// # Panics
    ///
    /// Panics if the shared font-system lock is poisoned.
    pub fn delete_selection(&mut self) {
        if let Some(editor) = &mut self.editor
            && editor.selection_bounds().is_some()
        {
            let mut font_sys = font_system().write().expect("Write font system");
            editor.action(font_sys.raw(), cosmic_text::Action::Backspace);
        }
    }

    /// # Panics
    ///
    /// Panics if the shared font-system lock is poisoned.
    pub fn insert_with_attrs(&mut self, data: &str) {
        self.blink_start.set(Instant::now());
        let attrs = self.current_attrs();
        let attrs_list = cosmic_text::AttrsList::new(&attrs);
        if let Some(editor) = &mut self.editor {
            let mut font_sys = font_system().write().expect("Write font system");
            editor.insert_string(data, Some(attrs_list));
            editor.shape_as_needed(font_sys.raw(), false);
        }
        self.sync_height();
    }

    pub fn sync_format_at_cursor(&mut self) {
        if let Some(editor) = &self.editor {
            let cursor = editor.cursor();
            editor.with_buffer(|buf| {
                if let Some(line) = buf.lines.get(cursor.line) {
                    let idx = if cursor.index > 0 {
                        cursor.index - 1
                    } else {
                        0
                    };
                    let a = line.attrs_list().get_span(idx);
                    self.bold = a.weight >= cosmic_text::Weight::BOLD;
                    self.italic = a.style == cosmic_text::Style::Italic;
                    self.underline = a.metadata == 1;
                    if let Some(c) = a.color_opt {
                        self.color = Color::from_rgba(
                            f32::from(c.r()) / 255.0,
                            f32::from(c.g()) / 255.0,
                            f32::from(c.b()) / 255.0,
                            f32::from(c.a()) / 255.0,
                        );
                    }
                    if let Some(m) = a.metrics_opt {
                        let metrics: cosmic_text::Metrics = m.into();
                        self.font_size = metrics.font_size;
                    }
                    if let cosmic_text::Family::Name(n) = a.family {
                        self.font_family = intern_str(n);
                    }
                }
            });
        }
    }

    fn current_attrs(&self) -> cosmic_text::Attrs<'static> {
        cosmic_text::Attrs::new()
            .family(cosmic_text::Family::Name(self.font_family))
            .weight(if self.bold {
                cosmic_text::Weight::BOLD
            } else {
                cosmic_text::Weight::NORMAL
            })
            .style(if self.italic {
                cosmic_text::Style::Italic
            } else {
                cosmic_text::Style::Normal
            })
            .metadata(usize::from(self.underline))
            .color(cosmic_text::Color::rgba(
                color_channel_u8(self.color.r),
                color_channel_u8(self.color.g),
                color_channel_u8(self.color.b),
                color_channel_u8(self.color.a),
            ))
            .metrics(cosmic_text::Metrics::new(
                self.font_size,
                self.font_size * LINE_HEIGHT_FACTOR,
            ))
    }

    fn sync_height(&mut self) {
        if let Some(editor) = &mut self.editor {
            let min_h = self.font_size * LINE_HEIGHT_FACTOR;
            let content_h: f32 = editor.with_buffer(content_height);
            self.bounding_box.height = content_h.max(min_h);

            editor.with_buffer_mut(|buf| {
                buf.set_size(Some(TEXT_INSET.mul_add(-2.0, self.bounding_box.width)), None);
                buf.set_scroll(cosmic_text::Scroll::default());
            });
        }
    }

    /// # Panics
    ///
    /// Panics if the shared font-system lock is poisoned.
    pub fn editor_action(&mut self, action: cosmic_text::Action) {
        self.blink_start.set(Instant::now());
        if let Some(editor) = &mut self.editor {
            let mut font_sys = font_system().write().expect("Write font system");
            editor.action(font_sys.raw(), action);
            editor.shape_as_needed(font_sys.raw(), false);
            drop(font_sys);

            // Fit box height to content
            let min_h = self.font_size * LINE_HEIGHT_FACTOR;
            let content_h: f32 = editor.with_buffer(content_height);
            self.bounding_box.height = content_h.max(min_h);

            editor.with_buffer_mut(|buf| {
                buf.set_size(Some(TEXT_INSET.mul_add(-2.0, self.bounding_box.width)), None);
                buf.set_scroll(cosmic_text::Scroll::default());
            });
        }
    }

    pub fn is_empty(&self) -> bool {
        self.editor
            .as_ref()
            .is_none_or(|ed| ed.with_buffer(|buf| buf.lines.iter().all(|l| l.text().is_empty())))
    }

    /// # Panics
    ///
    /// Panics if the shared font-system lock is poisoned.
    pub fn set_line_alignment(&mut self, align: Horizontal) {
        self.alignment = align;
        if let Some(editor) = &mut self.editor {
            let cursor = editor.cursor();
            let ct_align = match align {
                Horizontal::Left => cosmic_text::Align::Left,
                Horizontal::Center => cosmic_text::Align::Center,
                Horizontal::Right => cosmic_text::Align::Right,
            };
            let mut font_sys = font_system().write().expect("Write font system");
            editor.with_buffer_mut(|buf| {
                if let Some(line) = buf.lines.get_mut(cursor.line) {
                    line.set_align(Some(ct_align));
                }
                buf.shape_until_scroll(font_sys.raw(), false);
            });
        }
    }

    pub fn is_editing(&self) -> bool {
        self.state == TextEditState::Editing
    }

    pub fn hit_test_handle(&self, point: Point) -> TextDragHandle {
        let r = self.bounding_box;
        if r.width < 1.0 || r.height < 1.0 {
            return TextDragHandle::None;
        }

        let edge = 6.0;

        // Corners
        if near(point, Point::new(r.x, r.y), edge) {
            return TextDragHandle::TopLeft;
        }
        if near(point, Point::new(r.x + r.width, r.y), edge) {
            return TextDragHandle::TopRight;
        }
        if near(point, Point::new(r.x, r.y + r.height), edge) {
            return TextDragHandle::BottomLeft;
        }
        if near(point, Point::new(r.x + r.width, r.y + r.height), edge) {
            return TextDragHandle::BottomRight;
        }

        // Interior
        let inner = Rectangle::new(
            Point::new(r.x + edge, r.y + edge),
            Size::new(
                (r.width - edge * 2.0).max(0.0),
                (r.height - edge * 2.0).max(0.0),
            ),
        );
        if inner.contains(point) {
            return TextDragHandle::Move;
        }

        // Edges
        if (point.x - r.x).abs() < edge && point.y > r.y && point.y < r.y + r.height {
            return TextDragHandle::Left;
        }
        if (point.x - (r.x + r.width)).abs() < edge && point.y > r.y && point.y < r.y + r.height {
            return TextDragHandle::Right;
        }
        if (point.y - r.y).abs() < edge && point.x > r.x && point.x < r.x + r.width {
            return TextDragHandle::Top;
        }
        if (point.y - (r.y + r.height)).abs() < edge && point.x > r.x && point.x < r.x + r.width {
            return TextDragHandle::Bottom;
        }

        // Outside but close enough to count
        if r.contains(point) {
            return TextDragHandle::Move;
        }

        TextDragHandle::None
    }

    fn update_drag(&mut self, pos: Point, image_size: Size) {
        let delta_x = pos.x - self.drag_origin.x;
        let delta_y = pos.y - self.drag_origin.y;
        let reg = self.drag_start_box;

        let (mut x, mut y, mut w, mut h) = match self.active_handle {
            TextDragHandle::BottomRight => {
                (reg.x, reg.y, reg.width + delta_x, reg.height + delta_y)
            }
            TextDragHandle::TopLeft => (
                reg.x + delta_x,
                reg.y + delta_y,
                reg.width - delta_x,
                reg.height - delta_y,
            ),
            TextDragHandle::TopRight => (
                reg.x,
                reg.y + delta_y,
                reg.width + delta_x,
                reg.height - delta_y,
            ),
            TextDragHandle::BottomLeft => (
                reg.x + delta_x,
                reg.y,
                reg.width - delta_x,
                reg.height + delta_y,
            ),
            TextDragHandle::Top => (reg.x, reg.y + delta_y, reg.width, reg.height - delta_y),
            TextDragHandle::Bottom => (reg.x, reg.y, reg.width, reg.height + delta_y),
            TextDragHandle::Left => (reg.x + delta_x, reg.y, reg.width - delta_x, reg.height),
            TextDragHandle::Right => (reg.x, reg.y, reg.width + delta_x, reg.height),
            TextDragHandle::Move => (reg.x + delta_x, reg.y + delta_y, reg.width, reg.height),
            TextDragHandle::None => return,
        };

        // Enforce minimums, adjust position to keep opposite edge fixed
        if w < MIN_BOX_WIDTH {
            if matches!(
                self.active_handle,
                TextDragHandle::TopLeft | TextDragHandle::BottomLeft | TextDragHandle::Left
            ) {
                x = reg.x + reg.width - MIN_BOX_WIDTH;
            }
            w = MIN_BOX_WIDTH;
        }
        if h < MIN_BOX_HEIGHT {
            if matches!(
                self.active_handle,
                TextDragHandle::TopLeft | TextDragHandle::TopRight | TextDragHandle::Top
            ) {
                y = reg.y + reg.height - MIN_BOX_HEIGHT;
            }
            h = MIN_BOX_HEIGHT;
        }

        // The image-bounds clamp only applies to an axis-aligned (un-rotated) box. When rotated,
        // the reading box lives in a rotated frame and legitimately extends past image-aligned
        // bounds, so clamping it here would collapse the box and block resizing.
        if self.steps() == 0 {
            x = x.clamp(0.0, (image_size.width - w).max(0.0));
            y = y.clamp(0.0, (image_size.height - h).max(0.0));
            w = w.min(image_size.width - x);
            h = h.min(image_size.height - y);
        }

        self.bounding_box = Rectangle::new(Point::new(x, y), Size::new(w, h));

        if let Some(editor) = &mut self.editor {
            let mut font_sys = font_system().write().expect("Write font system");
            editor.with_buffer_mut(|buf| {
                buf.set_size(Some(TEXT_INSET.mul_add(-2.0, self.bounding_box.width)), None);
                buf.set_scroll(cosmic_text::Scroll::default());
            });
            editor.shape_as_needed(font_sys.raw(), false);
            drop(font_sys);

            let content_h: f32 = editor.with_buffer(content_height);

            let width_only = matches!(
                self.active_handle,
                TextDragHandle::Left | TextDragHandle::Right
            );

            if width_only {
                // Width handles
                let min_h = self.font_size * LINE_HEIGHT_FACTOR;
                self.bounding_box.height = content_h.max(min_h);
            } else {
                // Height handles
                self.bounding_box.height = self.bounding_box.height.max(content_h);
            }
        }
    }

    fn draw_bounding_box(&self, frame: &mut Frame<Renderer>, scale: f32) {
        let r = self.bounding_box;
        let accent: Color = cosmic::theme::active().cosmic().accent_color().into();
        let border_w = BORDER_WIDTH / scale;

        // Scale handles to ~15% of the shorter box side, clamped
        let short_side = r.width.min(r.height);
        let bar_long = (short_side * 0.10).clamp(4.0, 24.0);
        let bar_short = (bar_long * 0.25).max(1.5);

        // Border
        let inset = border_w / 2.0;
        frame.stroke(
            &Path::rectangle(
                Point::new(r.x + inset, r.y + inset),
                Size::new(r.width - border_w, r.height - border_w),
            ),
            Stroke::default().with_color(accent).with_width(border_w),
        );

        let left = r.x;
        let right = r.x + r.width;
        let top = r.y;
        let bottom = r.y + r.height;
        let mid_x = r.x + r.width / 2.0;
        let mid_y = r.y + r.height / 2.0;

        // Corner handles are L-shaped (two bars); edge handles are a single bar.
        // Each entry: (center, bar_w, bar_h, anchor_x, anchor_y).
        let handles = [
            (Point::new(left, top), bar_long, bar_short, 0.0, 0.0),
            (Point::new(left, top), bar_short, bar_long, 0.0, 0.0),
            (Point::new(right, top), bar_long, bar_short, 1.0, 0.0),
            (Point::new(right, top), bar_short, bar_long, 1.0, 0.0),
            (Point::new(left, bottom), bar_long, bar_short, 0.0, 1.0),
            (Point::new(left, bottom), bar_short, bar_long, 0.0, 1.0),
            (Point::new(right, bottom), bar_long, bar_short, 1.0, 1.0),
            (Point::new(right, bottom), bar_short, bar_long, 1.0, 1.0),
            (Point::new(mid_x, top), bar_long, bar_short, 0.5, 0.0),
            (Point::new(mid_x, bottom), bar_long, bar_short, 0.5, 1.0),
            (Point::new(left, mid_y), bar_short, bar_long, 0.0, 0.5),
            (Point::new(right, mid_y), bar_short, bar_long, 1.0, 0.5),
        ];
        for (center, w, h, anchor_x, anchor_y) in handles {
            draw_handle(frame, center, w, h, anchor_x, anchor_y, accent);
        }
    }

    fn draw_glyphs(
        &self,
        frame: &mut Frame<Renderer>,
        editor: &cosmic_text::Editor<'static>,
        origin: Point,
    ) {
        editor.with_buffer(|buffer| {
            for run in buffer.layout_runs() {
                if run.glyphs.is_empty() {
                    continue;
                }

                // Group adjacent glyphs that share attrs into a single text draw.
                let line = &buffer.lines[run.line_i];
                let attrs_list = line.attrs_list();
                let mut span_start = 0usize;

                while span_start < run.glyphs.len() {
                    let first = &run.glyphs[span_start];
                    let span_attrs = attrs_list.get_span(first.start);
                    let mut span_end = span_start + 1;

                    while span_end < run.glyphs.len() {
                        let g = &run.glyphs[span_end];
                        if attrs_list.get_span(g.start) != span_attrs {
                            break;
                        }
                        span_end += 1;
                    }

                    let last = &run.glyphs[span_end - 1];
                    let span_text = &run.text[first.start..last.end];
                    let bold = span_attrs.weight >= cosmic_text::Weight::BOLD;
                    let italic = span_attrs.style == cosmic_text::Style::Italic;
                    let underline = span_attrs.metadata == 1;
                    let span_color = span_attrs.color_opt.map_or(self.color, |c| {
                        Color::from_rgba(
                            f32::from(c.r()) / 255.0,
                            f32::from(c.g()) / 255.0,
                            f32::from(c.b()) / 255.0,
                            f32::from(c.a()) / 255.0,
                        )
                    });
                    let span_font_size = span_attrs.metrics_opt.map_or(self.font_size, |m| {
                        let metrics: cosmic_text::Metrics = m.into();
                        metrics.font_size
                    });
                    let span_family: &'static str = match span_attrs.family {
                        cosmic_text::Family::Name(n) if n != self.font_family => intern_str(n),
                        _ => self.font_family,
                    };

                    let baseline_offset =
                        span_font_size.mul_add(-LINE_HEIGHT_FACTOR, run.line_height);
                    let text = canvas::Text {
                        content: span_text.to_string(),
                        position: Point::new(
                            first.x + origin.x,
                            run.line_top + baseline_offset + origin.y,
                        ),
                        color: span_color,
                        size: span_font_size.into(),
                        font: Font {
                            family: font::Family::Name(span_family),
                            weight: if bold {
                                font::Weight::Bold
                            } else {
                                font::Weight::Normal
                            },
                            style: if italic {
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

                    if underline {
                        let uy = span_font_size
                            .mul_add(LINE_HEIGHT_FACTOR, run.line_top + baseline_offset)
                            + origin.y
                            - 1.0;
                        let ux = first.x + origin.x;
                        let uw = last.x + last.w - first.x;
                        frame.stroke(
                            &Path::line(Point::new(ux, uy), Point::new(ux + uw, uy)),
                            Stroke::default().with_color(span_color).with_width(1.0),
                        );
                    }

                    span_start = span_end;
                }
            }
        });
    }

    // reason: cursor pixel coordinates from cosmic-text are far below f32's exact-integer range.
    #[allow(clippy::cast_precision_loss)]
    fn draw_cursor(
        &self,
        frame: &mut Frame<Renderer>,
        editor: &cosmic_text::Editor<'static>,
        origin: Point,
        scale: f32,
    ) {
        let elapsed = self.blink_start.get().elapsed().as_millis();
        let cursor_visible = elapsed < 500 || (elapsed / 500).is_multiple_of(2);
        if self.state == TextEditState::Editing
            && cursor_visible
            && let Some((cx, cy)) = editor.cursor_position()
        {
            let cursor = editor.cursor();
            let (line_h, cursor_fs) = editor.with_buffer(|buf| {
                let mut lh = self.font_size * LINE_HEIGHT_FACTOR;
                let mut fs = self.font_size;
                for run in buf.layout_runs() {
                    if run.line_i == cursor.line {
                        lh = run.line_height;
                        if let Some(line) = buf.lines.get(run.line_i) {
                            let idx = if cursor.index > 0 {
                                cursor.index - 1
                            } else {
                                0
                            };
                            let a = line.attrs_list().get_span(idx);
                            if let Some(m) = a.metrics_opt {
                                let metrics: cosmic_text::Metrics = m.into();
                                fs = metrics.font_size;
                            }
                        }
                        break;
                    }
                }
                (lh, fs)
            });
            let offset = cursor_fs.mul_add(-LINE_HEIGHT_FACTOR, line_h).max(0.0);
            let cursor_x = cx as f32 + origin.x;
            let cursor_y = cy as f32 + offset + origin.y;
            let cursor_h = cursor_fs * LINE_HEIGHT_FACTOR;
            frame.fill_rectangle(
                Point::new(cursor_x, cursor_y),
                Size::new(1.0 / scale, cursor_h),
                Fill::from(self.color),
            );
        }
    }
}

impl ToolOperation for TextPreview {
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        self.last_scale.set(scale);

        // Edit in place: the box, glyphs, selection and caret are all authored in the upright
        // reading frame and rendered rotated about the box center, so a rotated text edits as it
        // will be saved. `rotation_steps == 0` skips the transform entirely.
        let steps = self.steps();
        // While resizing, rotate about the fixed start-of-drag center — the same pivot the input
        // un-rotation uses — so the box doesn't slide as its own center moves with the edge.
        let center = if self.state == TextEditState::Resizing {
            self.drag_start_box.center()
        } else {
            self.bounding_box.center()
        };
        if steps != 0 {
            frame.push_transform();
            frame.translate(Vector::new(center.x, center.y));
            frame.rotate(Radians(f32::from(steps) * f32::consts::FRAC_PI_2));
            frame.translate(Vector::new(-center.x, -center.y));
        }

        let show_frame = self.custom_dragged || !self.is_empty();
        if matches!(self.state, TextEditState::Editing | TextEditState::Resizing) && show_frame {
            self.draw_bounding_box(frame, scale);
        }
        if self.state == TextEditState::PlacingDrag && self.custom_dragged {
            self.draw_bounding_box(frame, scale);
        }

        if self.bounding_box.width >= 1.0 {
            let origin = Point::new(self.bounding_box.x + TEXT_INSET, self.bounding_box.y);

            if self.state == TextEditState::Editing && self.is_empty() {
                let placeholder = canvas::Text {
                    content: "Type here...".to_string(),
                    position: Point::new(origin.x, origin.y),
                    color: Color {
                        a: 0.4,
                        ..self.color
                    },
                    size: self.font_size.into(),
                    font: Font {
                        family: font::Family::Name(self.font_family),
                        ..Default::default()
                    },
                    max_width: f32::INFINITY,
                    line_height: LineHeight::Relative(LINE_HEIGHT_FACTOR),
                    align_x: Horizontal::Left.into(),
                    align_y: Vertical::Top,
                    shaping: Shaping::Advanced,
                };
                frame.fill_text(placeholder);
            }

            if let Some(editor) = &self.editor {
                // Selection highlights
                if let Some((start, end)) = editor.selection_bounds() {
                    let sel_color = Color::from_rgba(0.0, 0.5, 1.0, 0.4);
                    editor.with_buffer(|buffer| {
                        for run in buffer.layout_runs() {
                            for (x_start, x_width) in run.highlight(start, end) {
                                frame.fill_rectangle(
                                    Point::new(x_start + origin.x, run.line_top + origin.y),
                                    Size::new(x_width, run.line_height),
                                    Fill::from(sel_color),
                                );
                            }
                        }
                    });
                }

                self.draw_glyphs(frame, editor, origin);
                self.draw_cursor(frame, editor, origin, scale);
            }
        }

        if steps != 0 {
            frame.pop_transform();
        }
    }

    fn apply(&self, _image: &mut DynamicImage) {}

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        if self.bounding_box.width < 1.0 || self.is_empty() {
            return None;
        }

        let editor = self.editor.as_ref()?;
        let mut spans = Vec::new();
        let empty_span = || TextSpan {
            text: String::new(),
            bold: false,
            italic: false,
            underline: false,
            color: None,
            font_size: None,
            font_family: None,
            align: None,
        };

        editor.with_buffer(|buf| {
            let last_line = buf.lines.len().saturating_sub(1);
            for (line_idx, line) in buf.lines.iter().enumerate() {
                let text = line.text();
                let line_align = line.align().map(encode_align);

                if text.is_empty() {
                    let mut s = empty_span();
                    s.align = line_align;
                    spans.push(s);
                } else {
                    let attrs_list = line.attrs_list();
                    let bytes = text.as_bytes();
                    let mut i = 0;
                    let mut first = true;
                    while i < bytes.len() {
                        let a = attrs_list.get_span(i);
                        let mut j = i + 1;
                        while j < bytes.len() && attrs_list.get_span(j) == a {
                            j += 1;
                        }
                        while j < bytes.len() && !text.is_char_boundary(j) {
                            j += 1;
                        }
                        spans.push(TextSpan {
                            text: text[i..j].to_string(),
                            bold: a.weight >= cosmic_text::Weight::BOLD,
                            italic: a.style == cosmic_text::Style::Italic,
                            underline: a.metadata == 1,
                            color: a.color_opt.map(|c| {
                                [
                                    f32::from(c.r()) / 255.0,
                                    f32::from(c.g()) / 255.0,
                                    f32::from(c.b()) / 255.0,
                                    f32::from(c.a()) / 255.0,
                                ]
                            }),
                            font_size: a.metrics_opt.map(|m| {
                                let metrics: cosmic_text::Metrics = m.into();
                                metrics.font_size
                            }),
                            font_family: match a.family {
                                cosmic_text::Family::Name(n) => Some(intern_str(n)),
                                _ => None,
                            },
                            align: if first { line_align } else { None },
                        });
                        first = false;
                        i = j;
                    }
                }

                if line_idx < last_line {
                    spans.push(empty_span());
                }
            }
        });

        if spans.iter().all(|s| s.text.is_empty()) {
            return None;
        }

        Some(Box::new(TextOperation {
            position: self.bounding_box.position(),
            spans,
            color: self.color,
            font_size: self.font_size,
            font_family: self.font_family,
            alignment: self.alignment,
            bounding_box: rotated_footprint(self.bounding_box, self.rotation_steps),
            rotation_steps: self.rotation_steps,
        }))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    // reason: scaled hit-test coordinates are bounded by canvas size; truncation toward zero is the intended pixel quantization.
    #[allow(clippy::cast_possible_truncation)]
    fn on_press(&mut self, point: Point, _image_size: Size) -> mouse::Interaction {
        match self.state {
            TextEditState::Placing => {
                let h = TEXT_INSET.mul_add(2.0, self.font_size * LINE_HEIGHT_FACTOR);
                self.bounding_box = Rectangle::new(point, Size::new(DEFAULT_BOX_WIDTH, h));
                self.drag_origin = point;
                self.drag_start_box = self.bounding_box;
                self.custom_dragged = false;
                self.state = TextEditState::PlacingDrag;
                mouse::Interaction::Crosshair
            }
            TextEditState::PlacingDrag => mouse::Interaction::Crosshair,
            TextEditState::Editing => {
                // Un-rotate the press into the upright editing frame (identity when unrotated).
                let lp = self.to_local(point);
                let handle = self.hit_test_handle(lp);
                match handle {
                    TextDragHandle::None => {
                        // Outside box, app.rs will commit
                        mouse::Interaction::default()
                    }
                    TextDragHandle::Move => {
                        self.blink_start.set(Instant::now());
                        let x = (lp.x - self.bounding_box.x) as i32;
                        let y = (lp.y - self.bounding_box.y) as i32;

                        // Track click count for double/triple click
                        let now = std::time::Instant::now();
                        if let Some(prev) = self.last_click {
                            if now.duration_since(prev).as_millis() < 400 {
                                self.click_count = (self.click_count + 1).min(3);
                            } else {
                                self.click_count = 1;
                            }
                        } else {
                            self.click_count = 1;
                        }
                        self.last_click = Some(now);

                        match self.click_count {
                            2 => self.editor_action(cosmic_text::Action::DoubleClick { x, y }),
                            3 => self.editor_action(cosmic_text::Action::TripleClick { x, y }),
                            _ => self.editor_action(cosmic_text::Action::Click { x, y }),
                        }

                        self.mouse_selecting = true;
                        mouse::Interaction::Text
                    }
                    _ => {
                        self.active_handle = handle;
                        self.drag_origin = lp;
                        self.drag_start_box = self.bounding_box;
                        self.state = TextEditState::Resizing;
                        handle.rotated(self.steps()).cursor()
                    }
                }
            }
            TextEditState::Resizing => mouse::Interaction::default(),
        }
    }

    // reason: scaled drag coordinates are bounded by canvas size; truncation toward zero is the intended pixel quantization.
    #[allow(clippy::cast_possible_truncation)]
    fn on_drag(&mut self, point: Point, image_size: Size) {
        match self.state {
            TextEditState::PlacingDrag => {
                let dx = (point.x - self.drag_origin.x).abs();
                let dy = (point.y - self.drag_origin.y).abs();
                if dx > DRAG_THRESHOLD || dy > DRAG_THRESHOLD {
                    self.custom_dragged = true;
                    let x = self.drag_origin.x.min(point.x);
                    let y = self.drag_origin.y.min(point.y);
                    let w = (self.drag_origin.x - point.x).abs().max(MIN_BOX_WIDTH);
                    let h = (self.drag_origin.y - point.y).abs().max(MIN_BOX_HEIGHT);
                    self.bounding_box = Rectangle::new(Point::new(x, y), Size::new(w, h));
                }
            }
            TextEditState::Resizing => {
                // Un-rotate about the fixed start-of-drag center so the resize frame is stable.
                let lp = self.unrotate(point, self.drag_start_box.center());
                self.update_drag(lp, image_size);
            }
            TextEditState::Editing if self.mouse_selecting => {
                self.blink_start.set(Instant::now());
                let lp = self.to_local(point);
                let x = (lp.x - self.bounding_box.x) as i32;
                let y = (lp.y - self.bounding_box.y) as i32;
                self.editor_action(cosmic_text::Action::Drag { x, y });
            }
            _ => {}
        }
    }

    fn on_release(&mut self, _point: Point, _image_size: Size) {
        self.mouse_selecting = false;
        match self.state {
            TextEditState::PlacingDrag => {
                if self.custom_dragged {
                    // Bigger height drag = bigger font size (box is image-space)
                    self.font_size = snap_font_size(self.bounding_box.height / LINE_HEIGHT_FACTOR);
                }
                self.init_editor();
                self.state = TextEditState::Editing;
            }
            TextEditState::Resizing => {
                // The box was rendered about the fixed start-of-drag pivot; re-anchor it so that
                // rendering about its own center (in Editing) reproduces the same visual, with no
                // jump on release. (Identity when un-rotated.)
                if self.steps() != 0 {
                    let pivot = self.drag_start_box.center();
                    let c = self.bounding_box.center();
                    let t = i_minus_r(Vector::new(pivot.x - c.x, pivot.y - c.y), self.steps());
                    self.bounding_box.x += t.x;
                    self.bounding_box.y += t.y;
                }
                self.active_handle = TextDragHandle::None;
                self.state = TextEditState::Editing;
            }
            _ => {}
        }
    }

    fn cursor_at(&self, point: Point) -> mouse::Interaction {
        match self.state {
            TextEditState::Placing | TextEditState::PlacingDrag => mouse::Interaction::Crosshair,
            TextEditState::Editing => {
                let handle = self.hit_test_handle(self.to_local(point));
                if handle == TextDragHandle::None {
                    mouse::Interaction::Text
                } else {
                    handle.rotated(self.steps()).cursor()
                }
            }
            TextEditState::Resizing => self.active_handle.rotated(self.steps()).cursor(),
        }
    }
}

fn near(a: Point, b: Point, threshold: f32) -> bool {
    (a.x - b.x).abs() < threshold && (a.y - b.y).abs() < threshold
}

/// `v - R(v)` where `R` is the clockwise quarter-turn render rotation for `steps`. Used to
/// re-anchor a resized rotated box from the fixed-pivot render to its own-center render.
fn i_minus_r(v: Vector, steps: u8) -> Vector {
    let rv = match steps % 4 {
        1 => Vector::new(-v.y, v.x),
        2 => Vector::new(-v.x, -v.y),
        3 => Vector::new(v.y, -v.x),
        _ => v,
    };
    Vector::new(v.x - rv.x, v.y - rv.y)
}

fn draw_handle(
    frame: &mut Frame<Renderer>,
    center: Point,
    w: f32,
    h: f32,
    anchor_x: f32,
    anchor_y: f32,
    color: Color,
) {
    let rect = Rectangle::new(
        Point::new(
            w.mul_add(-anchor_x, center.x),
            h.mul_add(-anchor_y, center.y),
        ),
        Size::new(w, h),
    );
    frame.fill_rectangle(rect.position(), rect.size(), Fill::from(color));
}
