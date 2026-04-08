//! ImageGrid - A reactive grid widget
//!
//! Features:
//! - Built-in item rendering with proper centering
//! - Internal focus/selection tracking
//! - Mouse hover updates focus
//! - Keyboard navigation (arrow keys)
//! - Auto-scroll on focus change

use super::grid_core as core;
use cosmic::{
    Element, Renderer, Theme,
    iced::{
        Color, Length, Padding, Point, Rectangle, Size,
        advanced::{
            Clipboard, Layout, Shell, Widget,
            image::Renderer as ItemRenderer,
            layout::{Limits, Node},
            overlay,
            renderer::{self as iced_renderer, Quad, Renderer as QuadRenderer},
            svg::Renderer as SvgRenderer,
            widget::{Id, Operation, Tree},
        },
        event::Event,
        keyboard::{self, Key},
        mouse::{self, Button, Cursor},
    },
    widget::{container, image::Handle, scrollable},
};
use std::{cell::Cell, path::PathBuf};

/// An item in the image grid
#[derive(Debug, Clone)]
pub struct GridItem {
    pub path: PathBuf,
    pub handle: Option<Handle>,
    pub width: u32,
    pub height: u32,
}

impl GridItem {
    pub fn new(path: PathBuf, handle: Option<Handle>, width: u32, height: u32) -> Self {
        Self {
            path,
            handle,
            width,
            height,
        }
    }
}

/// Scroll request for auto-scrolling
#[derive(Debug, Clone, Copy)]
pub struct ScrollRequest {
    pub offset_y: f32,
}

/// Builder for ImageGrid
pub struct ImageGrid<'a, M> {
    inner: ImageGridInner<'a, M>,
    scrollable_id: Option<Id>,
    keyboard_nav_enabled: bool,
}

impl<'a, M: Clone + 'static> ImageGrid<'a, M> {
    pub fn new(items: Vec<GridItem>) -> Self {
        Self {
            inner: ImageGridInner {
                items,
                thumbnail_size: 128,
                focused_idx: None,
                selected_indicies: Vec::new(),
                padding: Padding::ZERO,
                col_spacing: 8,
                row_spacing: 8,
                width: Length::Fill,
                height: Length::Fill,
                on_focus: None,
                on_activate: None,
                on_scroll_request: None,
                last_layout: Cell::new((0, 0)),
                cached_cols: Cell::new(0),
                cached_row_height: Cell::new(0.0),
                keyboard_nav_enabled: true,
            },
            scrollable_id: None,
            keyboard_nav_enabled: true,
        }
    }

    /// Enable or disable keyboard navigation
    pub fn keyboard_navigation(mut self, enabled: bool) -> Self {
        self.keyboard_nav_enabled = enabled;
        self.inner.keyboard_nav_enabled = enabled;
        self
    }

    pub fn thumbnail_size(mut self, size: u32) -> Self {
        self.inner.thumbnail_size = size;
        self
    }

    pub fn focused(mut self, idx: Option<usize>) -> Self {
        self.inner.focused_idx = idx;
        self
    }

    pub fn selected(mut self, indices: Vec<usize>) -> Self {
        self.inner.selected_indicies = indices;
        self
    }

    pub fn get_focused(&self) -> Option<usize> {
        self.inner.focused_idx
    }

    pub fn set_focused(&mut self, idx: Option<usize>) {
        self.inner.focused_idx = idx;
    }

    pub fn get_selected(&self) -> &[usize] {
        &self.inner.selected_indicies
    }

    pub fn set_selected(&mut self, indices: Vec<usize>) {
        self.inner.selected_indicies = indices;
    }

    pub fn items(&self) -> &[GridItem] {
        &self.inner.items
    }

    pub fn set_items(&mut self, items: Vec<GridItem>) {
        self.inner.items = items;
    }

    pub fn items_mut(&mut self) -> &mut Vec<GridItem> {
        &mut self.inner.items
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.inner.padding = padding.into();
        self
    }

    pub fn spacing(mut self, spacing: u16) -> Self {
        self.inner.col_spacing = spacing;
        self.inner.row_spacing = spacing;
        self
    }

    pub fn column_spacing(mut self, spacing: u16) -> Self {
        self.inner.col_spacing = spacing;
        self
    }

    pub fn row_spacing(mut self, spacing: u16) -> Self {
        self.inner.row_spacing = spacing;
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.inner.height = height.into();
        self
    }

    pub fn scrollable(mut self, id: Id) -> Self {
        self.scrollable_id = Some(id);
        self
    }

    /// Callback when focus changes (hover or keyboard nav)
    pub fn on_focus<F>(mut self, f: F) -> Self
    where
        F: Fn(usize) -> M + 'a,
    {
        self.inner.on_focus = Some(Box::new(f));
        self
    }

    /// Callback when item is activated (click or Enter)
    pub fn on_activate<F>(mut self, f: F) -> Self
    where
        F: Fn(usize) -> M + 'a,
    {
        self.inner.on_activate = Some(Box::new(f));
        self
    }

    /// Callback when scroll is needed (for external scrollable container)
    pub fn on_scroll_request<F>(mut self, f: F) -> Self
    where
        F: Fn(ScrollRequest) -> M + 'a,
    {
        self.inner.on_scroll_request = Some(Box::new(f));
        self
    }

    pub fn into_element(self) -> Element<'a, M> {
        if let Some(scroll_id) = self.scrollable_id {
            scrollable(
                container({
                    let mut inner = self.inner;
                    inner.height = Length::Shrink;
                    inner
                })
                .padding(0),
            )
            .id(scroll_id)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            self.inner.into()
        }
    }
}

/// Convience function
pub fn image_grid<M: Clone + 'static>(items: Vec<GridItem>) -> ImageGrid<'static, M> {
    ImageGrid::new(items)
}

/// Inner widget that handles the actual rendering and events
struct ImageGridInner<'a, M> {
    items: Vec<GridItem>,
    thumbnail_size: u32,
    focused_idx: Option<usize>,
    selected_indicies: Vec<usize>,
    padding: Padding,
    col_spacing: u16,
    row_spacing: u16,
    width: Length,
    height: Length,
    on_focus: Option<Box<dyn Fn(usize) -> M + 'a>>,
    on_activate: Option<Box<dyn Fn(usize) -> M + 'a>>,
    on_scroll_request: Option<Box<dyn Fn(ScrollRequest) -> M + 'a>>,
    last_layout: Cell<(usize, u32)>,
    cached_cols: Cell<usize>,
    cached_row_height: Cell<f32>,
    keyboard_nav_enabled: bool,
}

impl<'a, M> ImageGridInner<'a, M> {
    fn grid_config(&self) -> core::GridConfig {
        let item_size = self.thumbnail_size as f32;
        let button_padding = self.col_spacing as f32;
        core::GridConfig {
            item_width: item_size + (button_padding * 2.0),
            column_spacing: self.col_spacing as f32,
            row_spacing: self.row_spacing as f32,
            min_cols: 1,
            max_cols: None,
            padding: self.padding,
        }
    }

    fn grid_metrics(&self, avail_width: f32) -> core::GridMetrics {
        let cfg = self.grid_config();
        let cols = core::calculate_columns(
            avail_width,
            cfg.item_width,
            cfg.column_spacing,
            cfg.min_cols,
            cfg.max_cols,
            self.items.len(),
        );
        let rows = self.items.len().div_ceil(cols);
        core::GridMetrics {
            cols,
            rows,
            row_height: cfg.item_width,
        }
    }

    fn item_at_position(&self, position: Point, bounds: Rectangle) -> Option<usize> {
        let cols = self.cached_cols.get();
        let row_height = self.cached_row_height.get();

        if cols == 0 || row_height <= 0.0 {
            return None;
        }

        let cfg = self.grid_config();
        let rows = self.items.len().div_ceil(cols);
        let local_x = position.x - bounds.x;
        let local_y = position.y - bounds.y;

        core::item_at_position(
            (local_x, local_y),
            cols,
            rows,
            cfg.item_width,
            row_height,
            cfg.column_spacing,
            cfg.row_spacing,
            cfg.padding,
            self.items.len(),
        )
    }

    fn is_selected(&self, idx: usize) -> bool {
        self.selected_indicies.contains(&idx)
    }
}

impl<'a, M: Clone + 'static> Widget<M, cosmic::Theme, Renderer> for ImageGridInner<'a, M> {
    fn children(&self) -> Vec<Tree> {
        // No child widgets - thumbnails are rendered directly
        Vec::new()
    }

    fn diff(&mut self, _tree: &mut Tree) {
        // No children to diff
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(&mut self, _tree: &mut Tree, _renderer: &Renderer, limits: &Limits) -> Node {
        if self.items.is_empty() {
            return Node::new(Size::ZERO);
        }

        let limits = limits.width(self.width).height(self.height);
        let max_size = limits.max();
        let avail_width = max_size.width - self.padding.x();

        let metrics = self.grid_metrics(avail_width);

        let total_height = (metrics.rows as f32 * metrics.row_height)
            + ((metrics.rows.saturating_sub(1)) as f32 * self.row_spacing as f32)
            + self.padding.y();

        self.cached_cols.set(metrics.cols);
        self.cached_row_height.set(metrics.row_height);
        self.last_layout.set((metrics.cols, metrics.row_height.to_bits()));

        let content_size = Size::new(avail_width + self.padding.x(), total_height);

        Node::new(limits.resolve(self.width, self.height, content_size))
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &iced_renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let cols = self.cached_cols.get();
        let row_height = self.cached_row_height.get();

        if cols == 0 || self.items.is_empty() {
            return;
        }

        let item_size = self.thumbnail_size as f32;
        let button_padding = self.col_spacing as f32;
        let cell_size = item_size + (button_padding * 2.0);

        let cosmic_theme = theme.cosmic();

        // Determine hovered item
        let hovered_idx = cursor.position().and_then(|pos| {
            if bounds.contains(pos) {
                self.item_at_position(pos, bounds)
            } else {
                None
            }
        });

        let radius: [f32; 4] = cosmic_theme.corner_radii.radius_s;
        let accent: Color = cosmic_theme.accent_color().into();

        for (idx, item) in self.items.iter().enumerate() {
            let row = idx / cols;
            let col = idx % cols;

            let x =
                bounds.x + self.padding.left + (col as f32 * (cell_size + self.col_spacing as f32));

            let y =
                bounds.y + self.padding.top + (row as f32 * (row_height + self.row_spacing as f32));

            let cell_bounds = Rectangle::new(Point::new(x, y), Size::new(cell_size, cell_size));

            let is_selected = self.is_selected(idx);
            let is_hovered = hovered_idx == Some(idx);

            // Draw cell background
            if is_hovered && !is_selected {
                renderer.fill_quad(
                    Quad {
                        bounds: cell_bounds,
                        border: cosmic::iced::Border {
                            radius: radius.into(),
                            width: 0.0,
                            color: Color::TRANSPARENT,
                        },
                        shadow: Default::default(),
                        snap: true,
                    },
                    Color::from_rgba(1.0, 1.0, 1.0, 0.1),
                );
            }

            // Draw thumbnail or placeholder
            let item_bounds = Rectangle::new(
                Point::new(x + button_padding, y + button_padding),
                Size::new(item_size, item_size),
            );

            if let Some(ref handle) = item.handle {
                let centered = core::calculate_centered_item_bounds(
                    item_bounds,
                    item.width as f32,
                    item.height as f32,
                );

                renderer.draw_image(
                    cosmic::iced::advanced::image::Image {
                        handle: handle.clone(),
                        filter_method: cosmic::iced::widget::image::FilterMethod::Linear,
                        rotation: cosmic::iced::Radians(0.0),
                        opacity: 1.0,
                        snap: true,
                        border_radius: radius.into(),
                    },
                    centered,
                    bounds,
                );
            } else {
                let placeholder_size = item_size / 2.0;
                let placeholder_bounds = Rectangle::new(
                    Point::new(
                        item_bounds.x + (item_size - placeholder_size) / 2.0,
                        item_bounds.y + (item_size - placeholder_size) / 2.0,
                    ),
                    Size::new(placeholder_size, placeholder_size),
                );

                renderer.fill_quad(
                    Quad {
                        bounds: placeholder_bounds,
                        border: cosmic::iced::Border {
                            radius: radius.into(),
                            ..Default::default()
                        },
                        shadow: Default::default(),
                        snap: true,
                    },
                    Color::from_rgba(0.5, 0.5, 0.5, 0.3),
                );
            }

            if is_selected {
                // Accent border - draw in a layer above the image
                let badge_w = 32.0;
                let badge_h = 20.0;
                let bg: Color = cosmic_theme.background.component.base.into();
                let corner_radii_m = cosmic_theme.corner_radii.radius_m;

                renderer.with_layer(bounds, |renderer| {
                    // Accent border
                    renderer.fill_quad(
                        Quad {
                            bounds: cell_bounds,
                            border: cosmic::iced::Border {
                                radius: radius.into(),
                                width: 2.0,
                                color: accent,
                            },
                            shadow: Default::default(),
                            snap: true,
                        },
                        Color::TRANSPARENT,
                    );

                    // Checkmark badge in bottom-left
                    let badge_bounds = Rectangle::new(
                        Point::new(
                            cell_bounds.x + 1.0,
                            cell_bounds.y + cell_bounds.height - badge_h - 1.0,
                        ),
                        Size::new(badge_w, badge_h),
                    );

                    renderer.fill_quad(
                        Quad {
                            bounds: badge_bounds,
                            border: cosmic::iced::Border {
                                radius: corner_radii_m.into(),
                                width: 1.0,
                                color: accent,
                            },
                            shadow: Default::default(),
                            snap: true,
                        },
                        bg,
                    );

                    let icon_handle =
                        cosmic::widget::icon::from_name("object-select-symbolic").handle();
                    if let cosmic::widget::icon::Data::Svg(svg_handle) = icon_handle.data {
                        let icon_size = 12.0;
                        let icon_bounds = Rectangle::new(
                            Point::new(
                                badge_bounds.x + (badge_w - icon_size) / 2.0,
                                badge_bounds.y + (badge_h - icon_size) / 2.0,
                            ),
                            Size::new(icon_size, icon_size),
                        );

                        renderer.draw_svg(
                            cosmic::iced::advanced::svg::Svg {
                                handle: svg_handle,
                                color: Some(accent),
                                rotation: cosmic::iced::Radians(0.0),
                                opacity: 1.0,
                                border_radius: [0.0; 4],
                            },
                            icon_bounds,
                            bounds,
                        );
                    }
                });
            }
        }
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {}
            Event::Mouse(mouse::Event::ButtonPressed(Button::Left)) => {
                if let Some(pos) = cursor.position()
                    && bounds.contains(pos)
                    && let Some(idx) = self.item_at_position(pos, bounds)
                {
                    if let Some(ref on_activate) = self.on_activate {
                        shell.publish(on_activate(idx));
                    }
                    shell.capture_event();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                if !self.keyboard_nav_enabled {
                    return;
                }

                let cols = self.cached_cols.get();
                if cols == 0 || self.items.is_empty() {
                    return;
                }

                let current = self.focused_idx.unwrap_or(0);
                let total = self.items.len();

                let new_idx = match key {
                    Key::Named(keyboard::key::Named::ArrowUp) => {
                        if current >= cols {
                            Some(current - cols)
                        } else {
                            None
                        }
                    }
                    Key::Named(keyboard::key::Named::ArrowDown) => {
                        if current + cols < total {
                            Some(current + cols)
                        } else {
                            None
                        }
                    }
                    Key::Named(keyboard::key::Named::Home) => Some(0),
                    Key::Named(keyboard::key::Named::End) => Some(total.saturating_sub(1)),
                    _ => None,
                };

                if let Some(new_idx) = new_idx {
                    self.focused_idx = Some(new_idx);
                    if let Some(ref on_activate) = self.on_activate {
                        shell.publish(on_activate(new_idx));
                    }
                    if let Some(ref on_focus) = self.on_focus {
                        shell.publish(on_focus(new_idx));
                    }

                    if let Some(ref on_scroll_request) = self.on_scroll_request {
                        let row_height = self.cached_row_height.get();
                        let row = new_idx / cols;
                        let row_spacing = self.row_spacing as f32;

                        let item_top = self.padding.top + (row as f32 * (row_height + row_spacing));
                        let item_bottom = item_top + row_height;

                        let scroll_offset = viewport.y - bounds.y;
                        let visible_top = scroll_offset;
                        let visible_bottom = scroll_offset + viewport.height;

                        if item_top < visible_top {
                            shell.publish(on_scroll_request(ScrollRequest { offset_y: item_top }));
                        } else if item_bottom > visible_bottom {
                            let new_offset = item_bottom - viewport.height;
                            shell.publish(on_scroll_request(ScrollRequest {
                                offset_y: new_offset.max(0.0),
                            }));
                        }
                    }
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();

        if let Some(pos) = cursor.position()
            && bounds.contains(pos)
            && self.item_at_position(pos, bounds).is_some()
        {
            return mouse::Interaction::Pointer;
        }
        mouse::Interaction::default()
    }

    fn operate(
        &mut self,
        _tree: &mut Tree,
        _layout: Layout<'_>,
        _renderer: &Renderer,
        _operation: &mut dyn Operation,
    ) {
    }

    fn overlay<'b>(
        &'b mut self,
        _tree: &'b mut Tree,
        _layout: Layout<'_>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        _translation: cosmic::iced::Vector,
    ) -> Option<overlay::Element<'b, M, cosmic::Theme, Renderer>> {
        None
    }
}

impl<'a, M: Clone + 'static> From<ImageGridInner<'a, M>> for Element<'a, M> {
    fn from(grid: ImageGridInner<'a, M>) -> Self {
        Element::new(grid)
    }
}
