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
}

impl<'a, Message: Clone + 'static> ResponsiveToolbar<'a, Message> {
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
    pub fn spacing(mut self, spacing: u16) -> Self {
        self.spacing = spacing;
        self
    }

    /// Kept for API compatibility, no longer used.
    #[must_use]
    pub fn overflow_open(self, _open: bool) -> Self {
        self
    }

    pub fn view<F>(self, _toggle_overflow: F) -> Element<'a, Message>
    where
        F: Fn() -> Message + 'static,
    {
        let spacing = cosmic::theme::active().cosmic().spacing;

        let start: Vec<_> = self.start.into_iter().map(|i| i.element).collect();
        let center: Vec<_> = self.center.into_iter().map(|i| i.element).collect();
        let end: Vec<_> = self.end.into_iter().map(|i| i.element).collect();

        let section = |items: Vec<Element<'a, Message>>| {
            row::with_children(items)
                .spacing(self.spacing)
                .align_y(Alignment::Center)
        };

        let has_start = !start.is_empty();
        let has_center = !center.is_empty();
        let has_end = !end.is_empty();

        match self.mode {
            ToolbarMode::Full => {
                // Single row: start | center | end
                let mut toolbar_row = row::with_capacity(8)
                    .align_y(Alignment::Center)
                    .spacing(self.spacing);

                if has_start {
                    toolbar_row = toolbar_row.push(section(start));
                }

                if has_start && (has_center || has_end) {
                    toolbar_row = toolbar_row
                        .push(divider::vertical::light().height(Length::Fixed(32.0)));
                }

                if has_center {
                    toolbar_row = toolbar_row.push(section(center));
                }

                if has_center && has_end {
                    toolbar_row = toolbar_row
                        .push(divider::vertical::light().height(Length::Fixed(32.0)));
                }

                if has_end {
                    toolbar_row = toolbar_row.push(section(end));
                }

                container(toolbar_row)
                    .padding([
                        spacing.space_xxs,
                        spacing.space_s,
                        spacing.space_xxs,
                        spacing.space_s,
                    ])
                    .height(Length::Shrink)
                    .class(theme::Container::Secondary)
                    .into()
            }
            ToolbarMode::Compact | ToolbarMode::Minimal => {
                // Two rows: top = start + end, bottom = center
                let mut top_row = row::with_capacity(8)
                    .align_y(Alignment::Center)
                    .spacing(self.spacing);

                if has_start {
                    top_row = top_row.push(section(start));
                }

                if has_start && has_end {
                    top_row = top_row
                        .push(divider::vertical::light().height(Length::Fixed(32.0)));
                }

                if has_end {
                    top_row = top_row.push(section(end));
                }

                let mut content = column()
                    .spacing(spacing.space_xxs)
                    .align_x(Alignment::Center)
                    .width(Length::Shrink);

                content = content.push(top_row);

                if has_center {
                    content = content.push(section(center));
                }

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
        }
    }
}

pub fn responsive_toolbar<'a, Message: Clone + 'static>(
    mode: ToolbarMode,
) -> ResponsiveToolbar<'a, Message> {
    ResponsiveToolbar::new(mode)
}
