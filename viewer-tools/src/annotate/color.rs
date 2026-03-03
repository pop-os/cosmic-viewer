use cosmic::iced::Color;

/// Preset annotation colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnnotateColor(pub Color);

impl AnnotateColor {
    pub fn presets() -> Vec<AnnotateColor> {
        vec![
            AnnotateColor(Color::WHITE),
            AnnotateColor(Color::from_rgb(1.0, 0.0, 0.0)), // Red
            AnnotateColor(Color::from_rgb(1.0, 0.65, 0.0)), // Orange
            AnnotateColor(Color::from_rgb(0.0, 1.0, 0.0)), // Green
            AnnotateColor(Color::from_rgb(0.0, 0.0, 1.0)), // Blue
            AnnotateColor(Color::BLACK),
        ]
    }
}

impl Default for AnnotateColor {
    fn default() -> Self {
        Self(Color::BLACK)
    }
}
