pub mod operation;
pub mod preview;

use cosmic::iced_wgpu::graphics::text::{cosmic_text, font_system};
pub use operation::TextOperation;
pub use preview::TextPreview;

#[derive(Debug, Clone)]
pub struct TextSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl TextSpan {
    pub fn new(bold: bool, italic: bool, underline: bool) -> Self {
        Self {
            text: String::new(),
            bold,
            italic,
            underline,
        }
    }
}

pub(crate) fn measure_span_width(
    text: &str,
    font_size: f32,
    font_family: &str,
    bold: bool,
    italic: bool,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }

    let mut font_sys = font_system().write().expect("Write font system");
    let attrs = cosmic_text::Attrs::new()
        .weight(if bold {
            cosmic_text::Weight::BOLD
        } else {
            cosmic_text::Weight::NORMAL
        })
        .style(if italic {
            cosmic_text::Style::Italic
        } else {
            cosmic_text::Style::Normal
        })
        .family(cosmic_text::Family::Name(font_family));

    let mut buffer_line = cosmic_text::BufferLine::new(
        text,
        cosmic_text::LineEnding::default(),
        cosmic_text::AttrsList::new(&attrs),
        cosmic_text::Shaping::Advanced,
    );

    let layout = buffer_line.layout(
        font_sys.raw(),
        font_size,
        None,
        cosmic_text::Wrap::None,
        cosmic_text::Ellipsize::None,
        None,
        8,
        cosmic_text::Hinting::Disabled,
    );

    layout.first().map_or(0.0, |line| line.w)
}
