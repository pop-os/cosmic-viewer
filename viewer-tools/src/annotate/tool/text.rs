pub mod operation;
pub mod preview;

use cosmic::iced::mouse;
use cosmic::iced_widget::graphics::text::cosmic_text;
use std::collections::HashSet;
use std::sync::Mutex;
pub use operation::TextOperation;
pub use preview::TextPreview;

static INTERNED: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);

pub(crate) fn intern_str(s: &str) -> &'static str {
    let mut guard = INTERNED.lock().unwrap();
    let set = guard.get_or_insert_with(HashSet::new);
    if let Some(&existing) = set.get(s) {
        return existing;
    }
    let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
    set.insert(leaked);
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
    pub fn cursor(&self) -> mouse::Interaction {
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

pub(crate) const BORDER_WIDTH: f32 = 1.5;

pub(crate) fn encode_align(align: cosmic_text::Align) -> u8 {
    match align {
        cosmic_text::Align::Left => 0,
        cosmic_text::Align::Center => 1,
        cosmic_text::Align::Right => 2,
        _ => 0,
    }
}

pub(crate) fn decode_align(val: u8) -> cosmic_text::Align {
    match val {
        1 => cosmic_text::Align::Center,
        2 => cosmic_text::Align::Right,
        _ => cosmic_text::Align::Left,
    }
}
pub(crate) const MIN_BOX_WIDTH: f32 = 40.0;
pub(crate) const MIN_BOX_HEIGHT: f32 = 20.0;
pub(crate) const DEFAULT_BOX_WIDTH: f32 = 150.0;
pub(crate) const LINE_HEIGHT_FACTOR: f32 = 1.2;
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

pub(crate) type SpanLine<'a> = (String, Vec<&'a TextSpan>, Option<u8>);

pub(crate) fn group_spans<'a>(spans: &'a [TextSpan]) -> Vec<SpanLine<'a>> {
    let mut lines: Vec<SpanLine<'a>> = vec![(String::new(), Vec::new(), None)];
    for span in spans {
        if span.text.is_empty() {
            lines.push((String::new(), Vec::new(), span.align));
        } else {
            let cur = lines.last_mut().unwrap();
            cur.0.push_str(&span.text);
            if cur.2.is_none() {
                cur.2 = span.align;
            }
            cur.1.push(span);
        }
    }
    lines
}

pub(crate) fn span_attrs<'a>(
    base: cosmic_text::Attrs<'a>,
    span: &TextSpan,
    font_scale: f32,
) -> cosmic_text::Attrs<'a> {
    let mut a = base
        .weight(if span.bold { cosmic_text::Weight::BOLD } else { cosmic_text::Weight::NORMAL })
        .style(if span.italic { cosmic_text::Style::Italic } else { cosmic_text::Style::Normal })
        .metadata(if span.underline { 1 } else { 0 });
    if let Some([r, g, b, alpha]) = span.color {
        a = a.color(cosmic_text::Color::rgba(
            (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, (alpha * 255.0) as u8,
        ));
    }
    if let Some(fs) = span.font_size {
        let scaled = fs * font_scale;
        a = a.metrics(cosmic_text::Metrics::new(scaled, scaled * LINE_HEIGHT_FACTOR));
    }
    if let Some(fam) = span.font_family {
        a = a.family(cosmic_text::Family::Name(fam));
    }
    a
}

pub(crate) fn build_buffer_line(
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

pub(crate) fn content_height(buf: &cosmic_text::Buffer) -> f32 {
    buf.layout_runs()
        .fold(0.0, |h, run| (run.line_top + run.line_height).max(h))
}

