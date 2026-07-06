// SPDX-License-Identifier: GPL-3.0-only

pub mod manager;

use crate::state::{CanvasImage, CanvasMessage, Interaction, ToolKind};
use cosmic::{
    Renderer, Theme,
    iced::advanced::image::Renderer as IcedRenderer,
    iced::widget::canvas::{Action, Event, Frame, Geometry, Program},
    iced::{
        Rectangle, Size, Vector,
        mouse::{self, Button, Cursor, Event as MouseEvent},
    },
    widget::canvas::Cache,
};
use viewer_tools::ToolOperation;

/// Per-pixel touchpad zoom sensitivity. Lower = slower.
const PIXEL_ZOOM_RATE: f32 = 0.0025;

/// Scale to fit an image within the frame, capped at 1.0: images smaller than the
/// frame render at actual size rather than being upscaled to fill (fit never
/// enlarges - so a tiny icon opens at 100%, not thousands of percent).
pub(crate) fn fit_scale(frame_w: f32, frame_h: f32, img_w: f32, img_h: f32) -> f32 {
    (frame_w / img_w).min(frame_h / img_h).min(1.0)
}

/// Fit scale without the 1.0 cap: images smaller than the frame enlarge to fill it.
/// Used during crop so a small image fills the viewport and the crop frame spans it.
pub(crate) fn fit_scale_uncapped(frame_w: f32, frame_h: f32, img_w: f32, img_h: f32) -> f32 {
    (frame_w / img_w).min(frame_h / img_h)
}

/// Per-frame view of the canvas state, built by `ViewportManager`.
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

impl ViewerCanvas<'_> {
    // reason: image dimensions are pixel counts; f32 precision is ample for geometry.
    #[allow(clippy::cast_precision_loss)]
    fn image_exceeds_bounds(&self, bounds: Rectangle) -> bool {
        self.image.is_some_and(|image| {
            let fit_scale = fit_scale(
                bounds.width,
                bounds.height,
                image.width as f32,
                image.height as f32,
            );
            let scale = self.zoom * fit_scale;
            image.width as f32 * scale > bounds.width || image.height as f32 * scale > bounds.height
        })
    }
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
                    if self.active_tool.is_none() && self.image_exceeds_bounds(bounds) {
                        *state = Interaction::Panning {
                            start: position,
                            start_pan: self.pan,
                        };
                    }
                    Some(Action::publish(CanvasMessage::ContextMenu(None)))
                }
                MouseEvent::CursorMoved { .. } => {
                    if let Interaction::Panning { start, start_pan } = state {
                        let delta = Vector::new(position.x - start.x, position.y - start.y);
                        Some(Action::publish(CanvasMessage::Pan(*start_pan + delta)))
                    } else {
                        *state = Interaction::None;
                        None
                    }
                }
                MouseEvent::ButtonReleased(Button::Left) => {
                    *state = Interaction::None;
                    Some(Action::capture())
                }
                MouseEvent::WheelScrolled { delta } => {
                    let msg = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => {
                            if *y < 0.0 {
                                CanvasMessage::ZoomOut
                            } else {
                                CanvasMessage::ZoomIn
                            }
                        }
                        mouse::ScrollDelta::Pixels { y, .. } => {
                            CanvasMessage::ZoomBy((1.0 + y * PIXEL_ZOOM_RATE).clamp(0.5, 2.0))
                        }
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
            let fit_scale = if self.active_tool == Some(ToolKind::Crop) {
                fit_scale_uncapped(
                    bounds.width,
                    bounds.height,
                    image.width as f32,
                    image.height as f32,
                )
            } else {
                fit_scale(
                    bounds.width,
                    bounds.height,
                    image.width as f32,
                    image.height as f32,
                )
            };
            let image_size = Size::new(image.width as f32, image.height as f32);

            let _ = renderer.load_image(&image.handle);

            if self.overlay_only {
                // Overlay layer. `center` is the outer frame's center; it must be captured
                // explicitly rather than read via `frame.center()` inside `with_clip`, because the
                // drafted clip frame reports its own (clip-sized) center.
                let center = frame.center();
                let effective_scale = self.zoom * fit_scale;

                let render = |frame: &mut Frame<Renderer>| {
                    frame.translate(Vector::new(center.x, center.y));
                    frame.translate(self.pan);
                    frame.scale(effective_scale);
                    frame.translate(Vector::new(
                        -(image.width as f32) / 2.0,
                        -(image.height as f32) / 2.0,
                    ));

                    for op in self.operations {
                        op.draw(frame, image_size, effective_scale);
                    }

                    if let Some(preview) = self.preview {
                        preview.draw(frame, image_size, effective_scale);
                    }
                };

                if self.active_tool == Some(ToolKind::Crop) {
                    // Crop frame: never clip - the handles extend past the image edge by design.
                    render(&mut frame);
                } else {
                    // Annotations: clip to the image rectangle so marks outside the (possibly
                    // cropped) image aren't painted into the dead area, matching the rasterized
                    // save where apply() can only write within the image buffer.
                    let img_clip = Rectangle {
                        x: center.x + self.pan.x - image.width as f32 * effective_scale / 2.0,
                        y: center.y + self.pan.y - image.height as f32 * effective_scale / 2.0,
                        width: image.width as f32 * effective_scale,
                        height: image.height as f32 * effective_scale,
                    };
                    frame.with_clip(img_clip, render);
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
