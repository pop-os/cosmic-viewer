// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    Element, Theme,
    iced::{
        Event, Length, Limits, Point, Rectangle, Renderer, Size, Vector,
        advanced::{
            Clipboard, Layout, Shell, layout::Node, overlay, renderer as iced_renderer,
            widget::Tree,
        },
        mouse::{self, Cursor},
    },
    theme,
    widget::{Operation, Widget, container},
};

use super::ToolbarMode;
use crate::ToolbarItem;

/// A three-section toolbar that lays out as a single row when it fits and
/// stacks into two rows (`start | end` on top, `center` below) when the
/// available width can't hold the single row.
pub struct ResponsiveToolbar<'a, Message> {
    start: Vec<ToolbarItem<'a, Message>>,
    center: Vec<ToolbarItem<'a, Message>>,
    end: Vec<ToolbarItem<'a, Message>>,
    spacing: u16,
    mode: ToolbarMode,
}

impl<'a, Message: Clone + 'static> ResponsiveToolbar<'a, Message> {
    #[must_use]
    pub fn new(mode: ToolbarMode) -> Self {
        let spacing = cosmic::theme::active().cosmic().spacing;
        Self {
            start: Vec::new(),
            center: Vec::new(),
            end: Vec::new(),
            spacing: spacing.space_xxs,
            mode,
        }
    }

    #[must_use]
    pub fn start(mut self, item: ToolbarItem<'a, Message>) -> Self {
        self.start.push(item);
        self
    }

    #[must_use]
    pub fn center(mut self, item: ToolbarItem<'a, Message>) -> Self {
        self.center.push(item);
        self
    }

    #[must_use]
    pub fn end(mut self, item: ToolbarItem<'a, Message>) -> Self {
        self.end.push(item);
        self
    }

    #[must_use]
    pub const fn spacing(mut self, spacing: u16) -> Self {
        self.spacing = spacing;
        self
    }

    #[must_use]
    pub fn view(self) -> Element<'a, Message> {
        let spacing = cosmic::theme::active().cosmic().spacing;

        let start: Vec<_> = self.start.into_iter().map(|i| i.element).collect();
        let center: Vec<_> = self.center.into_iter().map(|i| i.element).collect();
        let end: Vec<_> = self.end.into_iter().map(|i| i.element).collect();

        // Anything other than `Full` forces the stacked arrangement regardless
        // of measured width (crop/minimal callers rely on this).
        let force_stacked = self.mode != ToolbarMode::Full;

        let reflow = ReflowToolbar::new(
            start,
            center,
            end,
            f32::from(self.spacing),
            f32::from(spacing.space_xxs),
            force_stacked,
        );

        container(reflow)
            .padding([
                spacing.space_xxs,
                spacing.space_s,
                spacing.space_xxs,
                spacing.space_s,
            ])
            .width(Length::Shrink)
            .height(Length::Shrink)
            .class(theme::Container::Secondary)
            .into()
    }
}

#[must_use]
pub fn responsive_toolbar<'a, Message: Clone + 'static>(
    mode: ToolbarMode,
) -> ResponsiveToolbar<'a, Message> {
    ResponsiveToolbar::new(mode)
}

/// Reflow widget backing [`ResponsiveToolbar`].
///
/// Children are stored flat in `[start.., center.., end..]` order so the list
/// maps 1:1 onto `tree.children` for event/draw/overlay delegation. `layout()`
/// measures each section's natural width with the same flex layout a `row`
/// uses, sums them, and stacks only when the single row would overflow - so it
/// never clips and never stacks prematurely.
struct ReflowToolbar<'a, Message> {
    children: Vec<Element<'a, Message>>,
    n_start: usize,
    n_center: usize,
    spacing: f32,
    row_spacing: f32,
    force_stacked: bool,
}

impl<'a, Message> ReflowToolbar<'a, Message> {
    fn new(
        start: Vec<Element<'a, Message>>,
        center: Vec<Element<'a, Message>>,
        end: Vec<Element<'a, Message>>,
        spacing: f32,
        row_spacing: f32,
        force_stacked: bool,
    ) -> Self {
        let n_start = start.len();
        let n_center = center.len();
        let mut children = start;
        children.extend(center);
        children.extend(end);
        Self {
            children,
            n_start,
            n_center,
            spacing,
            row_spacing,
            force_stacked,
        }
    }
}

/// Width of a contiguous run of laid-out items placed in a row with `spacing`
/// between them.
#[allow(clippy::cast_precision_loss)] // reason: item counts are tiny, exact in f32
fn run_width(nodes: &[Node], spacing: f32) -> f32 {
    if nodes.is_empty() {
        return 0.0;
    }
    let items: f32 = nodes.iter().map(|n| n.size().width).sum();
    spacing.mul_add((nodes.len() - 1) as f32, items)
}

/// Tallest item in a run.
fn run_height(nodes: &[Node]) -> f32 {
    nodes.iter().map(|n| n.size().height).fold(0.0, f32::max)
}

/// Place items left-to-right from `x0`, each vertically centered within
/// `row_height`. Returns the x just past the last item (no trailing spacing).
fn place_run(nodes: &mut [Node], x0: f32, y0: f32, row_height: f32, spacing: f32) -> f32 {
    let mut x = x0;
    for node in nodes.iter_mut() {
        let height = node.size().height;
        node.move_to_mut(Point::new(x, y0 + (row_height - height) / 2.0));
        x += node.size().width + spacing;
    }
    if nodes.is_empty() { x0 } else { x - spacing }
}

/// Two-row design: `start | end` on the top row, `center` centered below.
fn place_two_rows(
    nodes: &mut [Node],
    n_start: usize,
    center_end: usize,
    spacing: f32,
    row_spacing: f32,
) -> Size {
    let start_w = run_width(&nodes[..n_start], spacing);
    let center_w = run_width(&nodes[n_start..center_end], spacing);
    let has_start = n_start > 0;
    let has_end = nodes.len() > center_end;
    let has_center = center_end > n_start;

    let top_height = run_height(&nodes[..n_start]).max(run_height(&nodes[center_end..]));
    place_run(&mut nodes[..n_start], 0.0, 0.0, top_height, spacing);
    let end_x0 = if has_start { start_w + spacing } else { 0.0 };
    let top_w = place_run(&mut nodes[center_end..], end_x0, 0.0, top_height, spacing).max(start_w);

    let total_w = top_w.max(center_w);
    let center_height = run_height(&nodes[n_start..center_end]);
    let bottom_y = if has_start || has_end {
        top_height + row_spacing
    } else {
        0.0
    };
    place_run(
        &mut nodes[n_start..center_end],
        (total_w - center_w) / 2.0,
        bottom_y,
        center_height,
        spacing,
    );

    let total_h = if has_center {
        bottom_y + center_height
    } else {
        top_height
    };
    Size::new(total_w, total_h)
}

/// Flow every item left-to-right, wrapping to a new row whenever the next item
/// would exceed `available`. Guarantees nothing is clipped (short of a single
/// item wider than the whole toolbar).
fn place_wrapped(nodes: &mut [Node], available: f32, spacing: f32, row_spacing: f32) -> Size {
    let mut rows: Vec<std::ops::Range<usize>> = Vec::new();
    let mut heights: Vec<f32> = Vec::new();
    let mut row_start = 0;
    let mut x = 0.0;
    let mut row_height = 0.0_f32;
    for (i, node) in nodes.iter().enumerate() {
        let size = node.size();
        if x > 0.0 && x + size.width > available {
            rows.push(row_start..i);
            heights.push(row_height);
            row_start = i;
            x = 0.0;
            row_height = 0.0;
        }
        x += size.width + spacing;
        row_height = row_height.max(size.height);
    }
    rows.push(row_start..nodes.len());
    heights.push(row_height);

    let mut y = 0.0;
    let mut total_w = 0.0_f32;
    for (range, height) in rows.into_iter().zip(heights) {
        total_w = total_w.max(place_run(&mut nodes[range], 0.0, y, height, spacing));
        y += height + row_spacing;
    }
    Size::new(total_w, (y - row_spacing).max(0.0))
}

impl<Message> Widget<Message, Theme, Renderer> for ReflowToolbar<'_, Message> {
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        // `limits.max().width` is the true available width - the layout system
        // has already excluded the nav bar and window chrome, so no manual
        // subtraction is needed here.
        let available = limits.max().width;
        let spacing = self.spacing;
        let row_spacing = self.row_spacing;
        let n_start = self.n_start;
        let center_end = n_start + self.n_center;

        // Lay out every item at its natural size to measure the fit.
        let child_limits = limits.loose();
        let mut nodes: Vec<Node> = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .map(|(child, state)| child.as_widget_mut().layout(state, renderer, &child_limits))
            .collect();

        let start_w = run_width(&nodes[..n_start], spacing);
        let center_w = run_width(&nodes[n_start..center_end], spacing);
        let end_w = run_width(&nodes[center_end..], spacing);

        let has_start = n_start > 0;
        let has_center = center_end > n_start;
        let has_end = nodes.len() > center_end;

        let present = u16::from(has_start) + u16::from(has_center) + u16::from(has_end);
        let single_w = spacing.mul_add(
            f32::from(present.saturating_sub(1)),
            start_w + center_w + end_w,
        );

        // Top row of the two-row design: `start | end`.
        let top_gap = if has_start && has_end { spacing } else { 0.0 };
        let top_w = start_w + end_w + top_gap;

        let size = if !self.force_stacked && single_w <= available {
            // One row: start | center | end.
            let row_height = run_height(&nodes);
            let width = place_run(&mut nodes, 0.0, 0.0, row_height, spacing);
            Size::new(width, row_height)
        } else if top_w <= available && center_w <= available {
            // Two rows, per design - but only when each row actually fits.
            place_two_rows(&mut nodes, n_start, center_end, spacing, row_spacing)
        } else {
            // Fall back to a wrapping flow so nothing is ever clipped.
            place_wrapped(&mut nodes, available, spacing, row_spacing)
        };

        Node::with_children(size, nodes)
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
        for ((child, state), c_layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            child.as_widget().draw(
                state,
                renderer,
                theme,
                style,
                c_layout.with_virtual_offset(layout.virtual_offset()),
                cursor,
                viewport,
            );
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
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        for ((child, state), c_layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                state,
                event,
                c_layout.with_virtual_offset(layout.virtual_offset()),
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), c_layout)| {
                child.as_widget().mouse_interaction(
                    state,
                    c_layout.with_virtual_offset(layout.virtual_offset()),
                    cursor,
                    viewport,
                    renderer,
                )
            })
            .max()
            .unwrap_or_default()
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, state), c_layout) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child.as_widget_mut().operate(
                    state,
                    c_layout.with_virtual_offset(layout.virtual_offset()),
                    renderer,
                    operation,
                );
            }
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message: 'a> From<ReflowToolbar<'a, Message>> for Element<'a, Message> {
    fn from(widget: ReflowToolbar<'a, Message>) -> Self {
        Element::new(widget)
    }
}
