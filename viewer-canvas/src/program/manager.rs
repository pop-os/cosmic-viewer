use super::ViewerCanvas;
use crate::{CanvasImage, CanvasMessage, state::ToolKind};
use cosmic::{
    Element, Theme,
    iced::{
        Event, Length, Limits, Point, Rectangle, Renderer, Size, Vector,
        advanced::renderer as iced_renderer,
        mouse::{self, Button, Cursor, Event as MouseEvent},
        overlay,
    },
    iced_core::{
        Clipboard, Layout, Renderer as CoreRenderer, Shell,
        layout::Node,
        widget::{Tree, tree},
    },
    widget::{self, Operation, Widget, image::Handle},
};
use image::DynamicImage;
use std::cell::Cell;
use viewer_tools::ToolOperation;

/// Orchestrator that owns the canvas state, edit operations, and undo/redo history.
pub struct ViewportManager {
    image: Option<CanvasImage>,
    working_image: Option<DynamicImage>,
    zoom: f32,
    pan: Vector,
    active_tool: Option<ToolKind>,
    pub tool_dragging: bool,
    /// Committed operations (undo stack)
    operations: Vec<Box<dyn ToolOperation>>,
    redo_stack: Vec<Box<dyn ToolOperation>>,
    // Live preview for the active tool
    active_preview: Option<Box<dyn ToolOperation>>,
    last_bounds: Cell<Rectangle>,
}

impl Default for ViewportManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewportManager {
    pub fn new() -> Self {
        Self {
            image: None,
            working_image: None,
            zoom: 1.0,
            pan: Vector::ZERO,
            active_tool: None,
            tool_dragging: false,
            operations: Vec::new(),
            redo_stack: Vec::new(),
            active_preview: None,
            last_bounds: Cell::new(Rectangle::new(Point::new(0.0, 0.0), Size::ZERO)),
        }
    }

    pub fn last_bounds(&self) -> &Cell<Rectangle> {
        &self.last_bounds
    }

    pub fn operations(&self) -> &Vec<Box<dyn ToolOperation>> {
        &self.operations
    }

    pub fn operations_mut(&mut self) -> &mut Vec<Box<dyn ToolOperation>> {
        &mut self.operations
    }

    pub fn set_image(&mut self, image: Option<CanvasImage>, base: Option<DynamicImage>) {
        self.image = image;
        self.working_image = base;
        self.zoom = 1.0;
        self.pan = Vector::ZERO;
        self.active_preview = None;
        self.active_tool = None;
    }

    pub fn image(&self) -> Option<&CanvasImage> {
        self.image.as_ref()
    }

    pub fn rebuild_image(&mut self, original: &DynamicImage) {
        let mut working = original.clone();
        for op in &self.operations {
            op.apply(&mut working);
        }

        let rgba = working.to_rgba8();
        let (width, height) = rgba.dimensions();
        let handle = Handle::from_rgba(width, height, rgba.into_raw());
        self.image = Some(CanvasImage {
            handle,
            width,
            height,
        });
        self.zoom = 1.0;
        self.pan = Vector::ZERO;

        self.operations.clear();
        self.redo_stack.clear();
        self.working_image = Some(working);
    }

    pub fn rebuild_display(&mut self) {
        if let Some(ref working) = self.working_image {
            let rgba = working.to_rgba8();
            let (width, height) = rgba.dimensions();
            let handle = Handle::from_rgba(width, height, rgba.into_raw());
            self.image = Some(CanvasImage {
                handle,
                width,
                height,
            });
            self.zoom = 1.0;
            self.pan = Vector::ZERO;
        }
    }

    pub fn working_image(&self) -> Option<&DynamicImage> {
        self.working_image.as_ref()
    }

    pub fn working_image_mut(&mut self) -> Option<&mut DynamicImage> {
        self.working_image.as_mut()
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
    }

    pub fn pan(&self) -> Vector {
        self.pan
    }

    pub fn set_pan(&mut self, pan: Vector) {
        self.pan = pan;
    }

    pub fn active_tool(&self) -> Option<ToolKind> {
        self.active_tool
    }

    pub fn set_active_tool(&mut self, tool: Option<ToolKind>) {
        self.active_tool = tool;
    }

    /// Commit an operation directly to the undo stack.
    // Clears redo stack.
    pub fn commit(&mut self, op: Box<dyn ToolOperation>) {
        self.operations.push(op);
        self.redo_stack.clear();
    }

    /// Commit the active preview via its own `commit()` method.
    /// Returns true if a commit was made.
    pub fn apply_tool(&mut self) -> bool {
        if let Some(ref preview) = self.active_preview
            && let Some(committed) = preview.commit()
        {
            self.operations.push(committed);
            self.redo_stack.clear();
            self.active_preview = None;
            self.active_tool = None;
            self.tool_dragging = false;
            return true;
        }

        false
    }

    /// Cancel the active tool. Clears the preview without committing.
    pub fn cancel_tool(&mut self) {
        self.active_preview = None;
        self.active_tool = None;
        self.tool_dragging = false;
    }

    /// Mutable access to the active preview for tool specific config.
    pub fn preview_mut(&mut self) -> Option<&mut (dyn ToolOperation + 'static)> {
        self.active_preview.as_deref_mut()
    }

    /// Undo the last committed operation.
    pub fn undo(&mut self) -> Option<&dyn ToolOperation> {
        if let Some(op) = self.operations.pop() {
            self.redo_stack.push(op);
            self.redo_stack.last().map(|op| op.as_ref())
        } else {
            None
        }
    }

    /// Redo the last undone operation.
    pub fn redo(&mut self) -> Option<&dyn ToolOperation> {
        if let Some(op) = self.redo_stack.pop() {
            self.operations.push(op);
            self.operations.last().map(|op| op.as_ref())
        } else {
            None
        }
    }

    /// Clear all operations and redo history.
    pub fn revert_all(&mut self) {
        self.operations.clear();
        self.redo_stack.clear();
        self.active_preview = None;
        self.working_image = None;
    }

    /// Set the active tool's live preview; not committed to undo stack.
    pub fn set_preview(&mut self, preview: Option<Box<dyn ToolOperation>>) {
        self.active_preview = preview;
    }

    pub fn preview_ref(&self) -> Option<&(dyn ToolOperation + 'static)> {
        self.active_preview.as_deref()
    }

    /// Convert a screen space point to image coordinates.
    pub fn screen_to_image(&self, point: Point, bounds: Rectangle) -> Option<Point> {
        let image = self.image.as_ref()?;
        let fit_scale =
            (bounds.width / image.width as f32).min(bounds.height / image.height as f32);
        let effective_scale = self.zoom * fit_scale;
        let center_x = bounds.width / 2.0;
        let center_y = bounds.height / 2.0;
        let img_x = (point.x - center_x - self.pan.x) / effective_scale + image.width as f32 / 2.0;
        let img_y = (point.y - center_y - self.pan.y) / effective_scale + image.height as f32 / 2.0;

        if img_x >= 0.0
            && img_y >= 0.0
            && img_x <= image.width as f32
            && img_y <= image.height as f32
        {
            Some(Point::new(img_x, img_y))
        } else {
            None
        }
    }

    // For ToolDrag - clamp to image bounds so strokes end at the edge
    pub fn screen_to_image_clamped(&self, point: Point, bounds: Rectangle) -> Option<Point> {
        let image = self.image.as_ref()?;
        let fit_scale =
            (bounds.width / image.width as f32).min(bounds.height / image.height as f32);
        let effective_scale = self.zoom * fit_scale;
        let center_x = bounds.width / 2.0;
        let center_y = bounds.height / 2.0;
        let img_x = (point.x - center_x - self.pan.x) / effective_scale + image.width as f32 / 2.0;
        let img_y = (point.y - center_y - self.pan.y) / effective_scale + image.height as f32 / 2.0;

        Some(Point::new(
            img_x.clamp(0.0, image.width as f32),
            img_y.clamp(0.0, image.height as f32),
        ))
    }

    /// Convert a image coordinate to a screen space point.
    pub fn image_to_screen(&self, point: Point, bounds: Rectangle) -> Option<Point> {
        let image = self.image.as_ref()?;
        let fit_scale =
            (bounds.width / image.width as f32).min(bounds.height / image.height as f32);
        let effective_scale = self.zoom * fit_scale;
        let center_x = bounds.width / 2.0;
        let center_y = bounds.width / 2.0;
        let screen_x =
            (point.x - image.width as f32 / 2.0) * effective_scale + center_x + self.pan.x;
        let screen_y =
            (point.y - image.height as f32 / 2.0) * effective_scale + center_y + self.pan.y;

        Some(Point::new(screen_x, screen_y))
    }

    /// Get the image dimensions as a Size.
    pub fn image_size(&self) -> Option<Size> {
        self.image
            .as_ref()
            .map(|img| Size::new(img.width as f32, img.height as f32))
    }

    pub fn can_undo(&self) -> bool {
        !self.operations.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn tool_dragging(&self) -> bool {
        self.tool_dragging
    }

    /// Build the element for use in `view()`
    pub fn element(&self) -> Element<'_, CanvasMessage> {
        Element::new(Viewport { manager: self })
    }
}

/// Private widget returned by `ViewportManager::element()`
/// Builds the canvas internally and wraps it with GPU clipping.
struct Viewport<'a> {
    manager: &'a ViewportManager,
}

impl<'a> Viewport<'a> {
    /// Image only canvas (no tool overlays).
    fn canvas_element(&self) -> Element<'_, CanvasMessage> {
        let mgr = self.manager;
        let canvas = ViewerCanvas {
            image: mgr.image.as_ref(),
            zoom: mgr.zoom,
            pan: mgr.pan,
            active_tool: mgr.active_tool,
            operations: &[],
            preview: None,
            overlay_only: false,
        };

        widget::canvas(canvas)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// Overlay only canvas (tool overlays, no image).
    fn overlay_element(&self) -> Element<'_, CanvasMessage> {
        let mgr = self.manager;
        let canvas = ViewerCanvas {
            image: mgr.image.as_ref(),
            zoom: mgr.zoom,
            pan: mgr.pan,
            active_tool: mgr.active_tool,
            operations: &mgr.operations,
            preview: mgr.active_preview.as_deref(),
            overlay_only: true,
        };

        widget::canvas(canvas)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

impl<'a> Widget<CanvasMessage, Theme, Renderer> for Viewport<'a> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        let mut element = self.canvas_element();
        let child = element
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let size = child.size();
        Node::with_children(size, vec![child])
    }

    fn tag(&self) -> tree::Tag {
        self.canvas_element().as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.canvas_element().as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(self.canvas_element())]
    }

    fn diff(&mut self, tree: &mut Tree) {
        let element = self.canvas_element();
        tree.diff_children(&mut [element]);
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
        self.manager.last_bounds.set(bounds);

        // Layer 1: Image
        renderer.with_layer(bounds, |renderer| {
            let element = self.canvas_element();
            element.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout.children().next().unwrap(),
                cursor,
                viewport,
            );
        });

        // Layer 2: Tool overlays
        if !self.manager.operations.is_empty() || self.manager.active_preview.is_some() {
            renderer.with_layer(bounds, |renderer| {
                let overlay = self.overlay_element();
                overlay.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    layout.children().next().unwrap(),
                    cursor,
                    viewport,
                );
            });
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, CanvasMessage>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        // Tool interaction takes priority
        if self.manager.active_tool.is_some()
            && let Event::Mouse(mouse_event) = event
            && let Some(position) = cursor.position_in(bounds)
        {
            match mouse_event {
                MouseEvent::ButtonPressed(Button::Left) => {
                    if let Some(pt) = self.manager.screen_to_image(position, bounds) {
                        shell.publish(CanvasMessage::ToolStart(pt));
                        shell.capture_event();
                        return;
                    }
                }
                MouseEvent::CursorMoved { .. } => {
                    if self.manager.tool_dragging
                        && let Some(pt) = self.manager.screen_to_image_clamped(position, bounds)
                    {
                        shell.publish(CanvasMessage::ToolDrag(pt));
                        shell.capture_event();
                        return;
                    }
                }
                MouseEvent::ButtonReleased(Button::Left) => {
                    if self.manager.tool_dragging {
                        shell.publish(CanvasMessage::ToolEnd);
                        shell.capture_event();
                        return;
                    }
                }
                _ => {}
            }
        }

        // Fall through to base canvas for zoom, context menu, etc.
        let mut element = self.canvas_element();
        element.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout.children().next().unwrap(),
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();

        // Handle tool cursors
        if self.manager.active_tool.is_some() {
            if let Some(position) = cursor.position_in(bounds)
                && let (Some(preview), Some(img_point)) = (
                    self.manager.active_preview.as_deref(),
                    self.manager.screen_to_image(position, bounds),
                )
            {
                return preview.cursor_at(img_point);
            }

            return mouse::Interaction::Crosshair;
        }

        // Delegate to base canvas for non-tool cursors
        let element = self.canvas_element();
        element.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().unwrap(),
            cursor,
            viewport,
            renderer,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let mut element = self.canvas_element();
        element.as_widget_mut().operate(
            &mut tree.children[0],
            layout.children().next().unwrap(),
            renderer,
            operation,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        _tree: &'b mut Tree,
        _layout: Layout<'_>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        _translation: Vector,
    ) -> Option<overlay::Element<'b, CanvasMessage, Theme, Renderer>> {
        None
    }
}

impl<'a> From<Viewport<'a>> for Element<'a, CanvasMessage> {
    fn from(widget: Viewport<'a>) -> Self {
        Element::new(widget)
    }
}
