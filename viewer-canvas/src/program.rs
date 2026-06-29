pub mod manager;

use crate::state::{CanvasImage, CanvasMessage, Interaction, ToolKind};
use cosmic::{
    Renderer, Theme,
    iced::{
        Rectangle, Size, Vector,
        mouse::{self, Button, Cursor, Event as MouseEvent},
    },
    iced::advanced::image::Renderer as IcedRenderer,
    iced::widget::canvas::{Action, Event, Frame, Geometry, Program},
    widget::canvas::Cache,
};
use viewer_tools::ToolOperation;

/// Per-frame view of the canvas state, built by `ViewportManger`.
pub struct ViewerCanvas<'a> {
    pub image: Option<&'a CanvasImage>,
    pub cache: &'a Cache,
    pub zoom: f32,
    pub pan: Vector,
    pub active_tool: Option<ToolKind>,
    pub operations: &'a [Box<dyn ToolOperation>],
    pub preview: Option<&'a dyn ToolOperation>,
    pub overlay_only: bool,
}

impl Program<CanvasMessage, Theme, Renderer> for ViewerCanvas<'_> {
    type State = Interaction;

    fn update(
        &self,
        state: &mut Interaction,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<CanvasMessage>> {
        let position = cursor.position_in(bounds)?;

        match event {
            Event::Mouse(mouse_event) => match mouse_event {
                MouseEvent::ButtonPressed(Button::Right) => {
                    Some(Action::publish(CanvasMessage::ContextMenu(Some(position))))
                }
                MouseEvent::ButtonPressed(Button::Left) => {
                    Some(Action::publish(CanvasMessage::ContextMenu(None)))
                }
                MouseEvent::CursorMoved { .. } => {
                    *state = Interaction::None;
                    None
                }
                MouseEvent::ButtonReleased(Button::Left) => {
                    *state = Interaction::None;
                    Some(Action::capture())
                }
                MouseEvent::WheelScrolled { delta } => {
                    let y = match delta {
                        mouse::ScrollDelta::Lines { y, .. }
                        | mouse::ScrollDelta::Pixels { y, .. } => y,
                    };

                    let msg = if *y < 0.0 {
                        CanvasMessage::ZoomOut
                    } else {
                        CanvasMessage::ZoomIn
                    };

                    Some(Action::publish(msg))
                }
                _ => None,
            },
            _ => None,
        }
    }

    // reason: image dimensions are pixel counts used for rendering geometry; f32 precision is ample.
    #[allow(clippy::cast_precision_loss)]
    fn draw(
        &self,
        _state: &Interaction,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        if let Some(image) = self.image {
            let fit_scale =
                (bounds.width / image.width as f32).min(bounds.height / image.height as f32);
            let image_size = Size::new(image.width as f32, image.height as f32);

            let _ = renderer.load_image(&image.handle);

            if self.overlay_only {
                // Overlay layer
                let center = frame.center();
                frame.translate(Vector::new(center.x, center.y));
                frame.translate(self.pan);
                frame.scale(self.zoom * fit_scale);
                frame.translate(Vector::new(
                    -(image.width as f32) / 2.0,
                    -(image.height as f32) / 2.0,
                ));

                let effective_scale = self.zoom * fit_scale;

                for op in self.operations {
                    op.draw(&mut frame, image_size, effective_scale);
                }

                if let Some(preview) = self.preview {
                    preview.draw(&mut frame, image_size, effective_scale);
                }
            } else {
                // Image layer
                let clip_region = Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width: bounds.width,
                    height: bounds.height,
                };

                frame.with_clip(clip_region, |frame| {
                    let center = frame.center();
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
