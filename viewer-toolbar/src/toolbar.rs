use crate::ItemPriority;
use cosmic::{
    Element,
    iced::{Alignment, Length},
    theme,
    widget::{button, column, container, divider, icon, popover, row, text},
};

use super::{ToolbarItem, ToolbarMode};

/// A three-section toolbar with vertical dividers between sections.
pub struct ResponsiveToolbar<'a, Message> {
    start: Vec<ToolbarItem<'a, Message>>,
    center: Vec<ToolbarItem<'a, Message>>,
    end: Vec<ToolbarItem<'a, Message>>,
    spacing: u16,
    mode: ToolbarMode,
    overflow_open: bool,
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
            overflow_open: false,
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

    #[must_use]
    pub fn overflow_open(mut self, open: bool) -> Self {
        self.overflow_open = open;
        self
    }

    pub fn view<F>(self, toggle_overflow: F) -> Element<'a, Message>
    where
        F: Fn() -> Message + 'static,
    {
        let Self {
            start,
            center,
            end,
            spacing: _,
            mode,
            overflow_open: _,
        } = self;

        let spacing = cosmic::theme::active().cosmic().spacing;

        // Collect visible and overflow items
        let mut visible_start = Vec::new();
        let mut visible_center = Vec::new();
        let mut visible_end = Vec::new();
        let mut overflow_items: Vec<(String, Option<&'static str>, Message)> = Vec::new();

        let min_priority = match mode {
            ToolbarMode::Full => ItemPriority::Optional,
            ToolbarMode::Compact => ItemPriority::Standard,
            ToolbarMode::Minimal => ItemPriority::Essential,
        };
        let should_show = |priority: ItemPriority| priority <= min_priority;

        for item in start {
            if should_show(item.priority) {
                visible_start.push(item.element);
            } else if let (Some(label), Some(msg)) = (item.overflow_label, item.overflow_message) {
                overflow_items.push((label, item.overflow_icon, msg));
            }
        }

        for item in center {
            if should_show(item.priority) {
                visible_center.push(item.element);
            } else if let (Some(label), Some(msg)) = (item.overflow_label, item.overflow_message) {
                overflow_items.push((label, item.overflow_icon, msg));
            }
        }

        for item in end {
            if should_show(item.priority) {
                visible_end.push(item.element);
            } else if let (Some(label), Some(msg)) = (item.overflow_label, item.overflow_message) {
                overflow_items.push((label, item.overflow_icon, msg));
            }
        }

        // Build toolbar
        let section = |items: Vec<Element<'a, Message>>, spacing: u16| {
            row::with_children(items)
                .spacing(spacing)
                .align_y(Alignment::Center)
        };

        let mut toolbar_row = row::with_capacity(8)
            .align_y(Alignment::Center)
            .spacing(self.spacing);

        let has_start = !visible_start.is_empty();
        let has_center = !visible_center.is_empty();
        let has_end = !visible_end.is_empty();
        let has_overflow = !overflow_items.is_empty();

        if has_start {
            toolbar_row = toolbar_row.push(section(visible_start, self.spacing));
        }

        if has_start && (has_center || has_end) {
            toolbar_row = toolbar_row.push(divider::vertical::light().height(Length::Fixed(24.0)));
        }

        if has_center {
            toolbar_row = toolbar_row.push(section(visible_center, self.spacing));
        }

        if has_center && has_end {
            toolbar_row = toolbar_row.push(divider::vertical::light().height(Length::Fixed(24.0)));
        }

        if has_end {
            toolbar_row = toolbar_row.push(section(visible_end, self.spacing));
        }

        // Add overflow menu button if needed
        if has_overflow {
            if has_start || has_center || has_end {
                toolbar_row =
                    toolbar_row.push(divider::vertical::light().height(Length::Fixed(24.0)));
            }

            let overflow_btn =
                button::icon(icon::from_name("view-more-symbolic")).on_press(toggle_overflow());

            let overflow_menu: Element<'a, Message> = column::with_children(
                overflow_items
                    .into_iter()
                    .map(|(label, icon_name, msg)| {
                        let mut btn_row = row::with_capacity(2).spacing(spacing.space_xs);
                        if let Some(icon_name) = icon_name {
                            btn_row = btn_row.push(icon::from_name(icon_name).size(16));
                        }

                        btn_row = btn_row.push(text::body(label));

                        button::custom(btn_row)
                            .class(theme::Button::MenuItem)
                            .on_press(msg)
                            .width(Length::Fill)
                            .into()
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(spacing.space_xxs)
            .padding(spacing.space_xs)
            .into();

            let mut overflow_popover = popover(overflow_btn);
            if self.overflow_open {
                overflow_popover = overflow_popover
                    .popup(overflow_menu)
                    .position(popover::Position::Bottom)
                    .on_close(toggle_overflow());
            }
            toolbar_row = toolbar_row.push(overflow_popover);
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
}

pub fn responsive_toolbar<'a, Message: Clone + 'static>(
    mode: ToolbarMode,
) -> ResponsiveToolbar<'a, Message> {
    ResponsiveToolbar::new(mode)
}
