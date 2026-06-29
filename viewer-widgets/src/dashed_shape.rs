use cosmic::{
    Renderer, Theme,
    iced::{
        Color, Point, Rectangle, Size, border,
        mouse::Cursor,
    },
    iced::widget::canvas::{self, Frame, Geometry, LineDash, Path, Program, Stroke},
};

#[derive(Debug, Clone, Copy)]
pub enum DashedShape {
    Circle,
    Pill,
    Square,
    RoundedS,
    RoundedM,
    RoundedL,
    Custom([f32; 4]),
}

impl DashedShape {
    fn to_radius(self, bounds: Size) -> border::Radius {
        let max_rad = bounds.width.min(bounds.height) / 2.0;
        match self {
            Self::Circle => max_rad.into(),
            Self::Pill => max_rad.min(160.0).into(),
            Self::Square => 0.0_f32.into(),
            Self::RoundedS => max_rad.min(8.0).into(),
            Self::RoundedM => max_rad.min(16.0).into(),
            Self::RoundedL => max_rad.min(32.0).into(),
            Self::Custom(rad) => [
                rad[0].min(max_rad),
                rad[1].min(max_rad),
                rad[2].min(max_rad),
                rad[3].min(max_rad),
            ]
            .into(),
        }
    }
}

pub struct DashedBorder {
    pub color: Color,
    pub stroke_width: f32,
    pub dash: f32,
    pub gap: f32,
    pub shape: DashedShape,
}

impl DashedBorder {
    #[must_use]
    pub const fn circle(color: Color, stroke_width: f32) -> Self {
        Self {
            color,
            stroke_width,
            dash: 3.0,
            gap: 3.0,
            shape: DashedShape::Circle,
        }
    }

    #[must_use]
    pub const fn pill(color: Color, stroke_width: f32) -> Self {
        Self {
            color,
            stroke_width,
            dash: 3.0,
            gap: 3.0,
            shape: DashedShape::Pill,
        }
    }

    #[must_use]
    pub const fn square(color: Color, stroke_width: f32) -> Self {
        Self {
            color,
            stroke_width,
            dash: 3.0,
            gap: 3.0,
            shape: DashedShape::Square,
        }
    }

    #[must_use]
    pub const fn rounded(color: Color, stroke_width: f32, shape: DashedShape) -> Self {
        Self {
            color,
            stroke_width,
            dash: 3.0,
            gap: 3.0,
            shape: match shape {
                // Guard against giving pre-defined shape and default to RoundedM
                DashedShape::Circle
                | DashedShape::Pill
                | DashedShape::Square
                | DashedShape::RoundedM => DashedShape::RoundedM,
                _ => shape,
            },
        }
    }

    #[must_use]
    pub const fn dash_pattern(mut self, dash: f32, gap: f32) -> Self {
        self.dash = dash;
        self.gap = gap;
        self
    }

    #[must_use]
    pub const fn dash(mut self, dash: f32) -> Self {
        self.dash = dash;
        self
    }

    #[must_use]
    pub const fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }
}

impl<M> Program<M, Theme, Renderer> for DashedBorder {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        let inset = self.stroke_width / 2.0;
        let inner_bounds = Size::new(
            bounds.width - self.stroke_width,
            bounds.height - self.stroke_width,
        );
        let segments = [self.dash, self.gap];
        let stroke = Stroke {
            style: canvas::stroke::Style::Solid(self.color),
            width: self.stroke_width,
            line_dash: LineDash {
                offset: 0,
                segments: &segments,
            },
            ..Stroke::default()
        };
        let path = if matches!(self.shape, DashedShape::Circle) {
            let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
            let radius = inner_bounds.width.min(inner_bounds.height) / 2.0;
            Path::circle(center, radius)
        } else {
            let radius = self.shape.to_radius(inner_bounds);
            Path::rounded_rectangle(Point::new(inset, inset), inner_bounds, radius)
        };

        frame.stroke(&path, stroke);

        vec![frame.into_geometry()]
    }
}
