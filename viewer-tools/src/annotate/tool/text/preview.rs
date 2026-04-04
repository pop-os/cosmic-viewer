use super::TextOperation;
use crate::{
    ToolOperation,
    annotate::tool::text::{TextSpan, measure_span_width},
};
use cosmic::{
    Renderer,
    iced::{
        Color, Font, Point, Size,
        alignment::{Horizontal, Vertical},
        font, mouse,
    },
    iced_core::text::{LineHeight, Shaping},
    iced_widget::canvas::{self, Frame, Path, Stroke},
};
use image::DynamicImage;
use std::any::Any;

#[derive(Debug, Clone, PartialEq)]
pub enum TextEditState {
    /// Waiting for user to click a position
    Placing,
    /// User has clicked - accepting keyboard input
    Editing,
}

#[derive(Debug, Clone)]
pub struct TextPreview {
    pub position: Option<Point>,
    pub spans: Vec<TextSpan>,
    pub color: Color,
    pub font_size: f32,
    pub font_family: &'static str,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub alignment: Horizontal,
    pub state: TextEditState,
}

impl TextPreview {
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
            position: None,
            spans: Vec::new(),
            color,
            font_size,
            font_family,
            bold,
            italic,
            underline,
            alignment,
            state: TextEditState::Placing,
        }
    }

    pub fn push_char(&mut self, c: char) {
        if self.state != TextEditState::Editing {
            return;
        }

        let matches = self.spans.last().is_some_and(|span| {
            span.bold == self.bold && span.italic == self.italic && span.underline == self.underline
        });

        if matches {
            self.spans.last_mut().unwrap().text.push(c);
        } else {
            let mut span = TextSpan::new(self.bold, self.italic, self.underline);
            span.text.push(c);
            self.spans.push(span);
        }
    }

    pub fn pop_char(&mut self) {
        if self.state != TextEditState::Editing {
            return;
        }

        if let Some(last) = self.spans.last_mut() {
            last.text.pop();
            if last.text.is_empty() {
                self.spans.pop();
            }
        }
    }

    pub fn full_text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty() || self.spans.iter().all(|span| span.text.is_empty())
    }

    pub fn is_editing(&self) -> bool {
        self.state == TextEditState::Editing
    }
}

impl ToolOperation for TextPreview {
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        let Some(pos) = self.position else { return };

        if self.is_empty() && self.state == TextEditState::Editing {
            // Draw placeholder
            let text = canvas::Text {
                content: "Type here...".to_string(),
                position: pos,
                color: Color::from_rgba(self.color.r, self.color.g, self.color.b, 0.4),
                size: (self.font_size / scale).into(),
                ..canvas::Text::default()
            };
            frame.fill_text(text);
            return;
        }

        let mut x_offset = 0.0;
        for span in &self.spans {
            if span.text.is_empty() {
                continue;
            }

            let text = canvas::Text {
                content: span.text.clone(),
                position: Point::new(pos.x + x_offset, pos.y),
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
                align_x: self.alignment.into(),
                align_y: Vertical::Top,
                shaping: Shaping::Advanced,
                ..Default::default()
            };

            frame.fill_text(text);

            let span_width = measure_span_width(
                &span.text,
                self.font_size,
                self.font_family,
                span.bold,
                span.italic,
            ) / scale;

            if span.underline {
                let underline_y = pos.y + self.font_size / scale;
                frame.stroke(
                    &Path::line(
                        Point::new(pos.x + x_offset, underline_y),
                        Point::new(pos.x + x_offset + span_width, underline_y),
                    ),
                    Stroke::default()
                        .with_color(self.color)
                        .with_width(1.0 / scale),
                );
            }

            x_offset += span_width;
        }

        // Cursor
        if self.state == TextEditState::Editing {
            let cursor_text = canvas::Text {
                content: "|".to_string(),
                position: Point::new(pos.x + x_offset, pos.y),
                color: self.color,
                size: (self.font_size / scale).into(),
                ..canvas::Text::default()
            };

            frame.fill_text(cursor_text);
        }
    }

    fn apply(&self, _image: &mut DynamicImage) {}

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        if let Some(pos) = self.position
            && !self.is_empty()
        {
            Some(Box::new(TextOperation::new(
                pos,
                self.spans.clone(),
                self.color,
                self.font_size,
                self.font_family,
                self.alignment.into(),
            )))
        } else {
            None
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn on_press(&mut self, point: Point, _image_size: Size) -> mouse::Interaction {
        match self.state {
            TextEditState::Placing => {
                self.position = Some(point);
                self.spans.clear();
                self.state = TextEditState::Editing;
                mouse::Interaction::Text
            }
            TextEditState::Editing => {
                // Click while editing - app will commit then re-place
                mouse::Interaction::Text
            }
        }
    }

    fn on_drag(&mut self, point: Point, _image_size: Size) {
        self.position = Some(point);
    }

    fn cursor_at(&self, _point: Point) -> mouse::Interaction {
        mouse::Interaction::Text
    }
}
