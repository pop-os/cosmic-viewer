use super::{handle::DragHandle, ratio::CropRatio};
use crate::{ToolOperation, crop::CropOperation};
use cosmic::{
    Renderer,
    iced::{Color, Point, Rectangle, Size},
    iced_widget::canvas::{Fill, Frame, Path, Stroke},
};
use image::DynamicImage;

const MIN_SIZE: f32 = 10.0;
const HANDLE_SIZE: f32 = 16.0;

/// Live crop selection state.
/// Transparent, which means never committed to undo/redo.
#[derive(Debug, Clone)]
pub struct CropSelection {
    /// Current selection rectangle in image coordinates.
    pub region: Rectangle,
    /// Active aspect ratio constraint.
    pub ratio: CropRatio,
    /// Which handle is being dragged.
    pub active_handle: DragHandle,
    /// Mouse position at drag start
    drag_origin: Point,
    /// Region snapshot at drag start.
    drag_start_region: Rectangle,
    /// Whether the selection is visible
    pub visible: bool,
}

impl CropSelection {
    pub fn new() -> Self {
        Self {
            region: Rectangle::default(),
            ratio: CropRatio::Custom,
            active_handle: DragHandle::None,
            drag_origin: Point::ORIGIN,
            drag_start_region: Rectangle::default(),
            visible: false,
        }
    }

    /// Activate with a specific ratio. For constrained ratios, initializes
    /// a centered crop frame that fills the image width.
    pub fn activate(&mut self, ratio: CropRatio, image_size: Size) {
        self.ratio = ratio;
        self.visible = true;

        if let Some(aspect) = ratio.resolve(image_size) {
            // Constrained: fit frame to image width, center vertically
            let width = image_size.width;
            let height = width / aspect;

            // If height exceeds image, fit to height instead
            let (width, height) = if height > image_size.height {
                (image_size.height * aspect, image_size.height)
            } else {
                (width, height)
            };

            self.region = Rectangle::new(
                Point::new(
                    (image_size.width - width) / 2.0,
                    (image_size.height - height) / 2.0,
                ),
                Size::new(width, height),
            );
        } else {
            // Custom: start with full image selected
            self.region = Rectangle::new(Point::ORIGIN, image_size);
        }
    }

    /// Change the ratio while preserving the selection center.
    pub fn set_ratio(&mut self, ratio: CropRatio, image_size: Size) {
        let center = Point::new(
            self.region.x + self.region.width / 2.0,
            self.region.y + self.region.height / 2.0,
        );

        self.ratio = ratio;

        if let Some(aspect) = ratio.resolve(image_size) {
            // Recompute from center, constrained image bounds
            let width = self.region.width;
            let height = width / aspect;

            let (width, height) = if height > image_size.height {
                (image_size.height * aspect, image_size.height)
            } else {
                (width, height)
            };

            let x = (center.x - width / 2.0).clamp(0.0, image_size.width - width);
            let y = (center.y - height / 2.0).clamp(0.0, image_size.height - height);

            self.region = Rectangle::new(Point::new(x, y), Size::new(width, height));
        }

        // Custom keeps the region as is
    }

    /// Begin a new selection from scratch at the given image-coordinate point
    pub fn start_new(&mut self, origin: Point) {
        self.region = Rectangle::new(origin, Size::ZERO);
        self.active_handle = DragHandle::BottomRight;
        self.drag_origin = origin;
        self.drag_start_region = self.region;
        self.visible = true;
    }

    /// Begin dragging an existing handle
    pub fn start_handle_drag(&mut self, handle: DragHandle, origin: Point) {
        self.active_handle = handle;
        self.drag_origin = origin;
        self.drag_start_region = self.region;
    }

    /// Update during drag. Enforces ratio constraint if active.
    pub fn update_drag(&mut self, pos: Point, image_size: Size) {
        let delta_x = pos.x - self.drag_origin.x;
        let delta_y = pos.y - self.drag_origin.y;
        let reg = self.drag_start_region;

        let (mut x, mut y, mut width, mut height) = match self.active_handle {
            DragHandle::BottomRight => (reg.x, reg.y, reg.width + delta_x, reg.height + delta_y),
            DragHandle::TopLeft => (
                reg.x + delta_x,
                reg.y + delta_y,
                reg.width - delta_x,
                reg.height - delta_y,
            ),
            DragHandle::TopRight => (
                reg.x,
                reg.y + delta_y,
                reg.width + delta_x,
                reg.height - delta_y,
            ),
            DragHandle::BottomLeft => (
                reg.x + delta_x,
                reg.y,
                reg.width - delta_x,
                reg.height + delta_y,
            ),
            DragHandle::Top => (reg.x, reg.y + delta_y, reg.width, reg.height - delta_y),
            DragHandle::Bottom => (reg.x, reg.y, reg.width, reg.height + delta_y),
            DragHandle::Left => (reg.x + delta_x, reg.y, reg.width - delta_x, reg.height),
            DragHandle::Right => (reg.x, reg.y, reg.width + delta_x, reg.height),
            DragHandle::Move => (reg.x + delta_x, reg.y + delta_y, reg.width, reg.height),
            DragHandle::None => return,
        };

        // Enforce min size
        width = width.max(MIN_SIZE);
        height = height.max(MIN_SIZE);

        // Enforce aspect ratio constraint
        if let Some(aspect) = self.ratio.resolve(image_size) {
            if !matches!(self.active_handle, DragHandle::Move) {
                // Width-dominant: adjust height to match ratio
                height = width / aspect;
                if height > image_size.height {
                    height = image_size.height;
                    width = height * aspect;
                }
            }
        }

        // Clamp to image bounds
        x = x.clamp(0.0, (image_size.width - width).max(0.0));
        y = y.clamp(0.0, (image_size.height - height).max(0.0));
        width = width.min(image_size.width - x);
        height = height.min(image_size.height - y);

        self.region = Rectangle::new(Point::new(x, y), Size::new(width, height));
    }

    /// Finish the drag. Returns true if the selection is valid.
    pub fn end_drag(&mut self) -> bool {
        self.active_handle = DragHandle::None;
        self.region.width >= MIN_SIZE && self.region.height >= MIN_SIZE
    }

    /// Hit-test a point against handles and interior
    pub fn hit_test(&self, point: Point) -> DragHandle {
        if !self.visible || self.region.width < MIN_SIZE {
            return DragHandle::None;
        }

        let region = self.region;
        let handle_size = HANDLE_SIZE;

        // Corners
        if Self::near(point, Point::new(region.x, region.y), handle_size) {
            return DragHandle::TopLeft;
        }

        if Self::near(
            point,
            Point::new(region.x + region.width, region.y),
            handle_size,
        ) {
            return DragHandle::TopRight;
        }

        if Self::near(
            point,
            Point::new(region.x, region.y + region.height),
            handle_size,
        ) {
            return DragHandle::BottomLeft;
        }

        if Self::near(
            point,
            Point::new(region.x + region.width, region.y + region.height),
            handle_size,
        ) {
            return DragHandle::BottomRight;
        }

        // Edges (only for Custom)
        if matches!(self.ratio, CropRatio::Custom) {
            if (point.x - region.x).abs() < handle_size
                && point.y > region.y
                && point.y < region.y + region.height
            {
                return DragHandle::Left;
            }

            if (point.x - (region.x + region.width)).abs() < handle_size
                && point.y > region.y
                && point.y < region.y + region.height
            {
                return DragHandle::Right;
            }

            if (point.y - region.y).abs() < handle_size
                && point.x > region.x
                && point.x < region.x + region.width
            {
                return DragHandle::Top;
            }

            if (point.y - (region.y + region.height)).abs() < handle_size
                && point.x > region.x
                && point.x < region.x + region.width
            {
                return DragHandle::Bottom;
            }
        }

        // Interior
        if region.contains(point) {
            return DragHandle::Move;
        }

        DragHandle::None
    }

    pub fn clear(&mut self) {
        self.visible = false;
        self.region = Rectangle::default();
        self.active_handle = DragHandle::None;
        self.ratio = CropRatio::Custom;
    }

    fn near(a: Point, b: Point, threshold: f32) -> bool {
        (a.x - b.x).abs() < threshold && (a.y - b.y).abs() < threshold
    }

    fn draw_handle(
        frame: &mut Frame<Renderer>,
        center: Point,
        handle_size: f32,
        anchor_x: f32,
        anchor_y: f32,
    ) {
        let rect = Rectangle::new(
            Point::new(
                center.x - handle_size * anchor_x,
                center.y - handle_size * anchor_y,
            ),
            Size::new(handle_size, handle_size),
        );

        frame.fill_rectangle(rect.position(), rect.size(), Fill::from(Color::WHITE));
        frame.stroke(
            &Path::rectangle(rect.position(), rect.size()),
            Stroke::default().with_color(Color::BLACK).with_width(1.0),
        );
    }
}

impl ToolOperation for CropSelection {
    fn draw(&self, frame: &mut Frame<Renderer>, image_size: Size, scale: f32) {
        if !self.visible || self.region.width < MIN_SIZE {
            return;
        }

        let region = self.region;
        let frame_size = image_size;
        let overlay_color = Color::from_rgba(0.0, 0.0, 0.0, 0.5);
        let border_width = 0.75 / scale;
        let grid_width = 1.0 / scale;
        let handle_size = HANDLE_SIZE / scale;

        // Dark overlay outside selection
        frame.fill_rectangle(
            Point::ORIGIN,
            Size::new(frame_size.width, region.y),
            Fill::from(overlay_color),
        );

        frame.fill_rectangle(
            Point::new(0.0, region.y + region.height),
            Size::new(
                frame_size.width,
                frame_size.height - region.y - region.height,
            ),
            Fill::from(overlay_color),
        );

        frame.fill_rectangle(
            Point::new(0.0, region.y),
            Size::new(region.x, region.height),
            Fill::from(overlay_color),
        );

        frame.fill_rectangle(
            Point::new(region.x + region.width, region.y),
            Size::new(frame_size.width - region.x - region.width, region.height),
            Fill::from(overlay_color),
        );

        // Selection border
        let inset = border_width / 2.0;
        frame.stroke(
            &Path::rectangle(
                Point::new(region.x + inset, region.y + inset),
                Size::new(region.width - border_width, region.height - border_width),
            ),
            Stroke::default()
                .with_color(Color::WHITE)
                .with_width(border_width),
        );

        // Rule of thirds grid lines
        let third_w = region.width / 3.0;
        let third_h = region.height / 3.0;
        let grid_stroke = Stroke::default()
            .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.4))
            .with_width(grid_width);

        for line in 1..3 {
            let x = region.x + third_w * line as f32;
            let y = region.y + third_h * line as f32;

            // Vertical grid line
            frame.stroke(
                &Path::line(
                    Point::new(x, region.y),
                    Point::new(x, region.y + region.height),
                ),
                grid_stroke,
            );

            // Horizontal grid line
            frame.stroke(
                &Path::line(
                    Point::new(region.x, y),
                    Point::new(region.x + region.width, y),
                ),
                grid_stroke,
            );
        }

        // Corner Handles
        Self::draw_handle(frame, Point::new(region.x, region.y), handle_size, 0.0, 0.0);
        Self::draw_handle(
            frame,
            Point::new(region.x + region.width, region.y),
            handle_size,
            1.0,
            0.0,
        );
        Self::draw_handle(
            frame,
            Point::new(region.x, region.y + region.height),
            handle_size,
            0.0,
            1.0,
        );
        Self::draw_handle(
            frame,
            Point::new(region.x + region.width, region.y + region.height),
            handle_size,
            1.0,
            1.0,
        );

        // Edge Handles
        Self::draw_handle(
            frame,
            Point::new(region.x + region.width / 2.0, region.y),
            handle_size,
            0.5,
            0.0,
        );
        Self::draw_handle(
            frame,
            Point::new(region.x + region.width / 2.0, region.y + region.height),
            handle_size,
            0.5,
            1.0,
        );
        Self::draw_handle(
            frame,
            Point::new(region.x, region.y + region.height / 2.0),
            handle_size,
            0.0,
            0.5,
        );
        Self::draw_handle(
            frame,
            Point::new(region.x + region.width, region.y + region.height / 2.0),
            handle_size,
            1.0,
            0.5,
        );
    }

    fn apply(&self, _image: &mut DynamicImage) {
        // Transparent - never modifies pixels.
    }

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        if self.visible && self.region.width >= MIN_SIZE && self.region.height >= MIN_SIZE {
            Some(Box::new(CropOperation::new(self.region)))
        } else {
            None
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
