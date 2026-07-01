use cosmic::{
    Element,
    iced::{Alignment, Length},
    theme,
    widget::{column, container, divider, row},
};

use super::ToolbarMode;
use crate::ToolbarItem;

/// A three-section toolbar that stacks into two rows when narrow.
pub struct ResponsiveToolbar<'a, Message> {
    start: Vec<ToolbarItem<'a, Message>>,
    center: Vec<ToolbarItem<'a, Message>>,
    end: Vec<ToolbarItem<'a, Message>>,
    spacing: u16,
    mode: ToolbarMode,
    available_width: Option<f32>,
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
            available_width: None,
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
    pub const fn available_width(mut self, width: f32) -> Self {
        self.available_width = Some(width);
        self
    }

    // `total_items` and `divider_count` are small toolbar element counts
    // (well under f32's 23-bit mantissa), so the casts are exact.
    #[allow(clippy::cast_precision_loss)] // reason: counts are tiny, always exact in f32
    fn estimated_width(
        total_items: usize,
        divider_count: usize,
        items_width: f32,
        item_spacing: f32,
        padding: f32,
    ) -> f32 {
        // Sum of per-item widths (variable-width items contribute their
        // real width via `width_hint`) plus inter-item spacing, the section
        // dividers, and the container padding.
        (divider_count as f32).mul_add(
            1.0 + item_spacing,
            (total_items as f32).mul_add(item_spacing, items_width),
        ) + padding
    }

    #[must_use]
    pub fn view(self) -> Element<'a, Message> {
        let spacing = cosmic::theme::active().cosmic().spacing;

        // Default assumed width for an item with no explicit hint:
        // a cosmic icon button renders ~32px square. Use that, not
        // `space_xl` (24), so even hint-less items are not under-counted.
        let default_item_width = f32::from(spacing.space_xl) + f32::from(spacing.space_s);

        // Sum the per-item width hints BEFORE the elements are moved out.
        let items_width: f32 = self
            .start
            .iter()
            .chain(self.center.iter())
            .chain(self.end.iter())
            .map(|i| i.width_hint.unwrap_or(default_item_width))
            .sum();

        let start: Vec<_> = self.start.into_iter().map(|i| i.element).collect();
        let center: Vec<_> = self.center.into_iter().map(|i| i.element).collect();
        let end: Vec<_> = self.end.into_iter().map(|i| i.element).collect();

        let has_start = !start.is_empty();
        let has_center = !center.is_empty();
        let has_end = !end.is_empty();

        let mode = if self.mode == ToolbarMode::Full {
            self.available_width.map_or(ToolbarMode::Full, |available| {
                let total_items = start.len() + center.len() + end.len();
                let divider_count =
                    usize::from(has_start) + usize::from(has_center) + usize::from(has_end);
                let needed = Self::estimated_width(
                    total_items,
                    divider_count.saturating_sub(1),
                    items_width,
                    f32::from(self.spacing),
                    f32::from(spacing.space_s) * 2.0,
                );
                if available < needed {
                    ToolbarMode::Compact
                } else {
                    ToolbarMode::Full
                }
            })
        } else {
            self.mode
        };

        let content = match mode {
            ToolbarMode::Full => Self::full_layout(start, center, end, self.spacing),
            ToolbarMode::Compact | ToolbarMode::Minimal => {
                Self::stacked_layout(start, center, end, self.spacing, spacing.space_xxs)
            }
        };

        container(content)
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

    /// Group items into a horizontally-spaced, vertically-centered row.
    fn section(items: Vec<Element<'a, Message>>, spacing: u16) -> Element<'a, Message> {
        row::with_children(items)
            .spacing(spacing)
            .align_y(Alignment::Center)
            .into()
    }

    /// Single row: `start | center | end`, dividers only between
    /// non-empty sections.
    fn full_layout(
        start: Vec<Element<'a, Message>>,
        center: Vec<Element<'a, Message>>,
        end: Vec<Element<'a, Message>>,
        spacing: u16,
    ) -> Element<'a, Message> {
        let has_start = !start.is_empty();
        let has_center = !center.is_empty();
        let has_end = !end.is_empty();

        let mut toolbar_row = row::with_capacity(8)
            .align_y(Alignment::Center)
            .spacing(spacing);

        if has_start {
            toolbar_row = toolbar_row.push(Self::section(start, spacing));
        }
        if has_start && (has_center || has_end) {
            toolbar_row = toolbar_row.push(divider::vertical::light().height(Length::Fixed(32.0)));
        }
        if has_center {
            toolbar_row = toolbar_row.push(Self::section(center, spacing));
        }
        if has_center && has_end {
            toolbar_row = toolbar_row.push(divider::vertical::light().height(Length::Fixed(32.0)));
        }
        if has_end {
            toolbar_row = toolbar_row.push(Self::section(end, spacing));
        }
        toolbar_row.into()
    }

    /// Two rows: top = `start | end`, bottom = `center`.
    fn stacked_layout(
        start: Vec<Element<'a, Message>>,
        center: Vec<Element<'a, Message>>,
        end: Vec<Element<'a, Message>>,
        spacing: u16,
        row_spacing: u16,
    ) -> Element<'a, Message> {
        let has_start = !start.is_empty();
        let has_center = !center.is_empty();
        let has_end = !end.is_empty();

        let mut top_row = row::with_capacity(8)
            .align_y(Alignment::Center)
            .spacing(spacing);

        if has_start {
            top_row = top_row.push(Self::section(start, spacing));
        }
        if has_start && has_end {
            top_row = top_row.push(divider::vertical::light().height(Length::Fixed(32.0)));
        }
        if has_end {
            top_row = top_row.push(Self::section(end, spacing));
        }

        let mut content = column![]
            .spacing(row_spacing)
            .align_x(Alignment::Center)
            .width(Length::Shrink)
            .push(top_row);

        if has_center {
            content = content.push(Self::section(center, spacing));
        }
        content.into()
    }
}

#[must_use]
pub fn responsive_toolbar<'a, Message: Clone + 'static>(
    mode: ToolbarMode,
) -> ResponsiveToolbar<'a, Message> {
    ResponsiveToolbar::new(mode)
}
