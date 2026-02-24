use cosmic::{
    iced::{Alignment, Length},
    widget::{self, container, divider, row, text},
    Element,
};

/// A three-section toolbar with vertical dividers between sections.
///
/// Accepts generic elements in start, center, and end regions.
/// Sections with no elements are omitted along with their adjacent divider.
pub struct Toolbar<'a, Message> {
    start: Vec<Element<'a, Message>>,
    center: Vec<Element<'a, Message>>,
    end: Vec<Element<'a, Message>>,
    spacing: u16,
}

impl<'a, Message: Clone + 'static> Toolbar<'a, Message> {
    pub fn new() -> Self {
        let spacing = cosmic::theme::active().cosmic().spacing;
        Self {
            start: Vec::new(),
            center: Vec::new(),
            end: Vec::new(),
            spacing: spacing.space_xxs,
        }
    }

    #[must_use]
    pub fn start(mut self, widget: impl Into<Element<'a, Message>>) -> Self {
        self.start.push(widget.into());
        self
    }

    #[must_use]
    pub fn center(mut self, widget: impl Into<Element<'a, Message>>) -> Self {
        self.center.push(widget.into());
        self
    }

    #[must_use]
    pub fn end(mut self, widget: impl Into<Element<'a, Message>>) -> Self {
        self.end.push(widget.into());
        self
    }

    #[must_use]
    pub fn spacing(mut self, spacing: u16) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn view(self) -> Element<'a, Message> {
        let has_start = !self.start.is_empty();
        let has_center = !self.center.is_empty();
        let has_end = !self.end.is_empty();

        let section = |items: Vec<Element<'a, Message>>, spacing: u16| {
            row::with_children(items)
                .spacing(spacing)
                .align_y(Alignment::Center)
        };

        let mut toolbar_row = row::with_capacity(7)
            .align_y(Alignment::Center)
            .spacing(self.spacing);

        if has_start {
            toolbar_row = toolbar_row.push(section(self.start, self.spacing));
        }

        toolbar_row = toolbar_row.push(divider::vertical::light().height(Length::Fixed(24.0)));

        if has_center {
            toolbar_row = toolbar_row.push(section(self.center, self.spacing));
        }

        toolbar_row = toolbar_row.push(divider::vertical::light().height(Length::Fixed(24.0)));

        if has_end {
            toolbar_row = toolbar_row.push(section(self.end, self.spacing));
        }

        let spacing = cosmic::theme::active().cosmic().spacing;

        container(toolbar_row)
            .padding([
                spacing.space_xxs,
                spacing.space_s,
                spacing.space_xxs,
                spacing.space_s,
            ])
            .height(Length::Shrink)
            .class(cosmic::theme::Container::Secondary)
            .into()
    }
}

impl<'a, Message: Clone + 'static> From<Toolbar<'a, Message>> for Element<'a, Message> {
    fn from(toolbar: Toolbar<'a, Message>) -> Self {
        toolbar.view()
    }
}

pub fn toolbar<'a, Message: Clone + 'static>() -> Toolbar<'a, Message> {
    Toolbar::new()
}
