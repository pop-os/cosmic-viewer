pub mod operation;
pub mod preview;

use cosmic::iced::mouse;
use cosmic::iced::advanced::graphics::text::cosmic_text;
pub use operation::TextOperation;
pub use preview::TextPreview;
use std::collections::HashSet;
use std::sync::Mutex;

static INTERNED: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);
pub const TEXT_INSET: f32 = 6.0;

/// # Panics
///
/// Panics if the intern table mutex is poisoned by a panic in another thread.
pub fn intern_str(s: &str) -> &'static str {
    let mut guard = INTERNED.lock().expect("intern table mutex poisoned");
    let set = guard.get_or_insert_with(HashSet::new);
    if let Some(&existing) = set.get(s) {
        return existing;
    }
    let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
    set.insert(leaked);
    drop(guard);
    leaked
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDragHandle {
    None,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Top,
    Bottom,
    Left,
    Right,
    Move,
}

impl TextDragHandle {
    #[must_use]
    pub const fn cursor(&self) -> mouse::Interaction {
        match self {
            Self::None => mouse::Interaction::Crosshair,
            Self::TopLeft | Self::BottomRight => mouse::Interaction::ResizingDiagonallyDown,
            Self::TopRight | Self::BottomLeft => mouse::Interaction::ResizingDiagonallyUp,
            Self::Top | Self::Bottom => mouse::Interaction::ResizingVertically,
            Self::Left | Self::Right => mouse::Interaction::ResizingHorizontally,
            Self::Move => mouse::Interaction::Grabbing,
        }
    }
}

pub const BORDER_WIDTH: f32 = 1.5;

/// Quantize a 0.0..=1.0 color component to an 8-bit channel, rounding to nearest
/// and clamping so out-of-gamut inputs cannot wrap.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // reason: clamped to 0.0..=255.0 then rounded, in-range for u8
pub fn color_channel_u8(component: f32) -> u8 {
    (component * 255.0).round().clamp(0.0, 255.0) as u8
}

pub const fn encode_align(align: cosmic_text::Align) -> u8 {
    match align {
        cosmic_text::Align::Center => 1,
        cosmic_text::Align::Right => 2,
        // Left and any future variant default to left alignment.
        _ => 0,
    }
}

pub const fn decode_align(val: u8) -> cosmic_text::Align {
    match val {
        1 => cosmic_text::Align::Center,
        2 => cosmic_text::Align::Right,
        _ => cosmic_text::Align::Left,
    }
}
pub const MIN_BOX_WIDTH: f32 = 40.0;
pub const MIN_BOX_HEIGHT: f32 = 20.0;
pub const DEFAULT_BOX_WIDTH: f32 = 200.0;
pub const LINE_HEIGHT_FACTOR: f32 = 1.2;
pub const FONT_SIZE_PRESETS: [f32; 7] = [12.0, 16.0, 20.0, 24.0, 32.0, 40.0, 64.0];
const DRAG_THRESHOLD: f32 = 5.0;

#[derive(Debug, Clone)]
pub struct TextSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Option<[f32; 4]>,
    pub font_size: Option<f32>,
    pub font_family: Option<&'static str>,
    pub align: Option<u8>,
}

pub type SpanLine<'a> = (String, Vec<&'a TextSpan>, Option<u8>);

pub fn group_spans<'a>(spans: &'a [TextSpan]) -> Vec<SpanLine<'a>> {
    let mut lines: Vec<SpanLine<'a>> = vec![(String::new(), Vec::new(), None)];
    for span in spans {
        if span.text.is_empty() {
            lines.push((String::new(), Vec::new(), span.align));
        } else {
            // `lines` is seeded with one entry and never popped, so it is non-empty.
            let cur = lines.last_mut().expect("lines is never empty");
            cur.0.push_str(&span.text);
            if cur.2.is_none() {
                cur.2 = span.align;
            }
            cur.1.push(span);
        }
    }
    lines
}

pub fn span_attrs<'a>(
    base: cosmic_text::Attrs<'a>,
    span: &TextSpan,
    font_scale: f32,
) -> cosmic_text::Attrs<'a> {
    let mut a = base
        .weight(if span.bold {
            cosmic_text::Weight::BOLD
        } else {
            cosmic_text::Weight::NORMAL
        })
        .style(if span.italic {
            cosmic_text::Style::Italic
        } else {
            cosmic_text::Style::Normal
        })
        .metadata(usize::from(span.underline));
    if let Some([r, g, b, alpha]) = span.color {
        a = a.color(cosmic_text::Color::rgba(
            color_channel_u8(r),
            color_channel_u8(g),
            color_channel_u8(b),
            color_channel_u8(alpha),
        ));
    }
    if let Some(fs) = span.font_size {
        let scaled = fs * font_scale;
        a = a.metrics(cosmic_text::Metrics::new(
            scaled,
            scaled * LINE_HEIGHT_FACTOR,
        ));
    }
    if let Some(fam) = span.font_family {
        a = a.family(cosmic_text::Family::Name(fam));
    }
    a
}

pub fn build_buffer_line(
    text: &str,
    attrs_list: cosmic_text::AttrsList,
    align: Option<u8>,
) -> cosmic_text::BufferLine {
    let mut line = cosmic_text::BufferLine::new(
        text,
        cosmic_text::LineEnding::default(),
        attrs_list,
        cosmic_text::Shaping::Advanced,
    );
    if let Some(a) = align {
        line.set_align(Some(decode_align(a)));
    }
    line
}

pub fn content_height(buf: &cosmic_text::Buffer) -> f32 {
    buf.layout_runs()
        .fold(0.0, |h, run| (run.line_top + run.line_height).max(h))
}

pub fn snap_font_size(target: f32) -> f32 {
    FONT_SIZE_PRESETS
        .iter()
        .copied()
        .min_by(|a, b| (a - target).abs().total_cmp(&(b - target).abs()))
        .unwrap_or(24.0)
}
