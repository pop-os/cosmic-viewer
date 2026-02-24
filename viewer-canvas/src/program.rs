use crate::state::{CanvasImage, CanvasMessage, Interaction, ToolKind};
use cosmic::{
    Element, Renderer, Theme,
    iced::{
        Length, Rectangle, Size, Vector,
        advanced::{
            Clipboard, Layout, Shell, Widget,
            layout::{Limits, Node},
            overlay,
            renderer::{self as iced_renderer},
            widget::{Operation, Tree, tree},
        },
        event::{self, Event as IcedEvent},
        mouse::{self, Button, Cursor, Event as MouseEvent},
    },
    iced_core::Renderer as CoreRenderer,
    iced_widget::canvas::{Event, Frame, Geometry, Program, event::Status},
};

pub struct ViewerCanvas {
    pub image: Option<CanvasImage>,
    pub zoom: f32,
    pub pan: Vector,
    pub active_tool: Option<ToolKind>,
}

impl Default for ViewerCanvas {
    fn default() -> Self {
        Self {
            image: None,
            zoom: 1.0,
            pan: Vector::ZERO,
            active_tool: None,
        }
    }
}

impl ViewerCanvas {
    /// Clamp pan so the image edges never go past the canvas edges
    fn clamp_pan(&self, pan: Vector, bounds: Rectangle) -> Vector {
        let Some(ref image) = self.image else {
            return Vector::ZERO;
        };

        let fit_scale =
            (bounds.width / image.width as f32).min(bounds.height / image.height as f32);
        let effective_scale = self.zoom * fit_scale;

        let half_width = image.width as f32 / 2.0 * effective_scale;
        let half_height = image.height as f32 / 2.0 * effective_scale;
        let center_x = bounds.width / 2.0;
        let center_y = bounds.height / 2.0;

        let max_pan_x = (half_width - center_x).max(0.0);
        let max_pan_y = (half_height - center_y).max(0.0);

        Vector::new(
            pan.x.clamp(-max_pan_x, max_pan_x),
            pan.y.clamp(-max_pan_y, max_pan_y),
        )
    }
}

impl Program<CanvasMessage, Theme, Renderer> for ViewerCanvas {
    type State = Interaction;

    fn update(
        &self,
        state: &mut Interaction,
        event: Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> (Status, Option<CanvasMessage>) {
        let Some(position) = cursor.position_in(bounds) else {
            return (Status::Ignored, None);
        };

        match event {
            Event::Mouse(mouse_event) => match mouse_event {
                MouseEvent::ButtonPressed(Button::Right) => (
                    Status::Captured,
                    Some(CanvasMessage::ContextMenu(Some(position))),
                ),
                MouseEvent::ButtonPressed(Button::Left) => {
                    // ToDo: Tool Interactions added here

                    (Status::Captured, None)
                }
                MouseEvent::CursorMoved { .. } => {
                    *state = Interaction::None;
                    (Status::Ignored, None)
                }
                MouseEvent::ButtonReleased(Button::Left) => {
                    *state = Interaction::None;
                    (Status::Captured, None)
                }
                MouseEvent::WheelScrolled { delta } => {
                    let y = match delta {
                        mouse::ScrollDelta::Lines { y, .. }
                        | mouse::ScrollDelta::Pixels { y, .. } => y,
                    };

                    let msg = if y < 0.0 {
                        CanvasMessage::ZoomOut
                    } else {
                        CanvasMessage::ZoomIn
                    };

                    (Status::Captured, Some(msg))
                }
                _ => (Status::Ignored, None),
            },
            _ => (Status::Ignored, None),
        }
    }

    fn draw(
        &self,
        _state: &Interaction,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        if let Some(ref image) = self.image {
            let clip_region = Rectangle {
                x: 0.0,
                y: 0.0,
                width: bounds.width,
                height: bounds.height,
            };

            frame.with_clip(clip_region, |frame| {
                let center = frame.center();
                let fit_scale =
                    (bounds.width / image.width as f32).min(bounds.height / image.height as f32);

                frame.translate(Vector::new(center.x, center.y));
                frame.translate(self.pan);
                frame.scale(self.zoom * fit_scale);

                let img_rect = Rectangle {
                    x: -(image.width as f32) / 2.0,
                    y: -(image.height as f32) / 2.0,
                    width: image.width as f32,
                    height: image.height as f32,
                };

                frame.draw_image(img_rect, &image.handle);
            });
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Interaction,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        if !cursor.is_over(bounds) {
            return mouse::Interaction::default();
        }

        match state {
            Interaction::Panning { .. } => mouse::Interaction::Grabbing,
            Interaction::None => {
                if self.active_tool.is_some() {
                    mouse::Interaction::Crosshair
                } else {
                    mouse::Interaction::default()
                }
            }
        }
    }
}

pub struct ClipElement<'a, Message> {
    content: Element<'a, Message>,
}

impl<'a, Message> ClipElement<'a, Message> {
    pub fn new(content: impl Into<Element<'a, Message>>) -> Self {
        Self {
            content: content.into(),
        }
    }
}

impl<Message> Widget<Message, Theme, Renderer> for ClipElement<'_, Message> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        let child = self.content.as_widget().layout(tree, renderer, limits);
        let size = child.size();
        Node::with_children(size, vec![child])
    }

    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&mut self, tree: &mut Tree) {
        self.content.as_widget_mut().diff(tree)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &iced_renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        renderer.with_layer(bounds, |renderer| {
            self.content.as_widget().draw(
                tree,
                renderer,
                theme,
                style,
                layout.children().next().unwrap(),
                cursor,
                viewport,
            );
        });
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: IcedEvent,
        layout: Layout<'_>,
        cursor: Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) -> event::Status {
        self.content.as_widget_mut().on_event(
            tree,
            event,
            layout.children().next().unwrap(),
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        )
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            tree,
            layout.children().next().unwrap(),
            cursor,
            viewport,
            renderer,
        )
    }

    fn operate(
        &self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget().operate(
            tree,
            layout.children().next().unwrap(),
            renderer,
            operation,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            tree,
            layout.children().next().unwrap(),
            renderer,
            translation,
        )
    }
}

impl<'a, Message: 'a> From<ClipElement<'a, Message>> for Element<'a, Message> {
    fn from(clip: ClipElement<'a, Message>) -> Self {
        Element::new(clip)
    }
}
