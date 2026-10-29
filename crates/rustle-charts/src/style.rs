use rustle_graphics::Color;

/// How the axes and labels of a chart look.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartStyle {
    /// Horizontal guide lines.
    pub grid: Color,
    /// Axis labels.
    pub label: Color,
    pub label_size: f32,
    /// The color behind the chart, for the holes in markers.
    pub surface: Color,
    /// Start the value axis at zero. Off, it starts at the lowest value, so
    /// small changes fill the height.
    pub zero_baseline: bool,
}

impl Default for ChartStyle {
    fn default() -> Self {
        Self {
            grid: Color::rgba(255, 255, 255, 18),
            label: Color::rgb(113, 113, 122),
            label_size: 11.0,
            surface: Color::rgb(17, 17, 19),
            zero_baseline: true,
        }
    }
}

/// One line of a chart, with its values from left to right.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub values: Vec<f32>,
    pub color: Color,
    /// Fill the space under the line with a fading tint of `color`.
    pub fill: bool,
    pub width: f32,
}

impl Series {
    pub fn new(values: Vec<f32>, color: Color) -> Self {
        Self {
            values,
            color,
            fill: false,
            width: 1.5,
        }
    }

    /// Fills the space under the line.
    pub fn filled(mut self) -> Self {
        self.fill = true;
        self
    }

    pub fn thickness(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}
