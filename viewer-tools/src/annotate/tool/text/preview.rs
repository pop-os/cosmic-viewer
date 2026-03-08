use super::TextOperation;
use crate::ToolOperation;
use cosmic::{
    Renderer,
    iced::{Color, Point, Size, mouse},
    iced_widget::{
        canvas::{self, Frame},
        graphics::text::{cosmic_text, font_system},
    },
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
    pub content: String,
    pub color: Color,
    pub font_size: f32,
    pub state: TextEditState,
}

impl TextPreview {
    pub fn new(color: Color, font_size: f32) -> Self {
        Self {
            position: None,
            content: String::new(),
            color,
            font_size,
            state: TextEditState::Placing,
        }
    }

    pub fn push_char(&mut self, c: char) {
        if self.state == TextEditState::Editing {
            self.content.push(c);
        }
    }

    pub fn pop_char(&mut self) {
        if self.state == TextEditState::Editing {
            self.content.pop();
        }
    }

    pub fn is_editing(&self) -> bool {
        self.state == TextEditState::Editing
    }

    /// Rough text widgh approximation for cursor placement.
    fn measure_text_width(&self) -> f32 {
        if self.content.is_empty() {
            return 0.0;
        }

        let mut font_sys = font_system().write().expect("Write font system");

        let mut buffer_line = cosmic_text::BufferLine::new(
            &self.content,
            cosmic_text::LineEnding::default(),
            cosmic_text::AttrsList::new(&cosmic_text::Attrs::new()),
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

        layout.first().map_or(0.0, |line| line.w)
    }
}

impl ToolOperation for TextPreview {
    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        let Some(pos) = self.position else { return };

        let display = if self.content.is_empty() && self.state == TextEditState::Editing {
            "Type here..."
        } else {
            &self.content
        };

        if display.is_empty() {
            return;
        }

        let text = canvas::Text {
            content: display.to_string(),
            position: pos,
            color: if self.content.is_empty() {
                Color::from_rgba(self.color.r, self.color.g, self.color.b, 0.4)
            } else {
                self.color
            },
            size: (self.font_size / scale).into(),
            ..canvas::Text::default()
        };

        frame.fill_text(text);

        // Draw cursor when editing
        if self.state == TextEditState::Editing {
            let text_width = self.measure_text_width() / scale;
            let cursor_text = canvas::Text {
                content: "|".to_string(),
                position: Point::new(pos.x + text_width, pos.y),
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
            && !self.content.trim().is_empty()
        {
            Some(Box::new(TextOperation::new(
                pos,
                self.content.clone(),
                self.color,
                self.font_size,
            )))
        } else {
            None
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn on_press(&mut self, point: Point, _image_size: Size) -> mouse::Interaction {
        match self.state {
            TextEditState::Placing => {
                self.position = Some(point);
                self.content.clear();
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
        if self.state == TextEditState::Placing {
            self.position = Some(point);
        }
    }

    fn cursor_at(&self, _point: Point) -> mouse::Interaction {
        mouse::Interaction::Text
    }
}
