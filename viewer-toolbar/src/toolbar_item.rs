use super::ItemPriority;
use cosmic::Element;

/// A toolbar item with priority for responsive handling
pub struct ToolbarItem<'a, Message> {
    pub element: Element<'a, Message>,
    pub priority: ItemPriority,
    /// Optional menu representation for overflow
    pub overflow_label: Option<String>,
    pub overflow_icon: Option<&'static str>,
    pub overflow_message: Option<Message>,
}

impl<'a, Message> ToolbarItem<'a, Message>
where
    Message: Clone + 'static,
{
    pub fn new(element: impl Into<Element<'a, Message>>) -> Self {
        Self {
            element: element.into(),
            priority: ItemPriority::Standard,
            overflow_label: None,
            overflow_icon: None,
            overflow_message: None,
        }
    }

    #[must_use]
    pub fn priority(mut self, priority: ItemPriority) -> Self {
        self.priority = priority;
        self
    }

    #[must_use]
    pub fn overflow(
        mut self,
        label: impl Into<String>,
        icon_name: Option<&'static str>,
        message: Message,
    ) -> Self {
        self.overflow_label = Some(label.into());
        self.overflow_icon = icon_name;
        self.overflow_message = Some(message);
        self
    }
}
