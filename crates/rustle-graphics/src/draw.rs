use rustle_geometry::Rect;

use crate::color::*;
use crate::image::*;
use crate::path::*;

/// An outline drawn inside the edge of a rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub width: f32,
    pub color: Color,
}

/// How the lines of a piece of text line up within its width.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum TextAlign {
    /// The side the text starts on: left for left-to-right text, right for right-to-left.
    #[default]
    Start,
    Left,
    Center,
    Right,
}

/// Which family of fonts text is set in.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum FontFamily {
    #[default]
    SansSerif,
    Serif,
    Monospace,
    /// An installed font by name, falling back to the default if it is missing.
    Named(&'static str),
}

/// A run of text in one style, within a line of rich text.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledSpan {
    pub text: String,
    pub color: Color,
    /// From 100 (thin) to 900 (black); 400 is normal.
    pub font_weight: u16,
    pub font_family: FontFamily,
}

/// A backend-independent drawing instruction produced by layout, in paint order.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// A filled rectangle with rounded corners and an optional border.
    Rect {
        rect: Rect,
        color: Color,
        corner_radius: f32,
        border: Option<Border>,
    },
    /// A rounded rectangle blending from `from` to `to` along `angle`, in radians
    /// clockwise from pointing right.
    Gradient {
        rect: Rect,
        from: Color,
        to: Color,
        angle: f32,
        corner_radius: f32,
    },
    /// A ring segment inside the square `rect`, `thickness` wide, covering
    /// `sweep` (0 to 1) of the circle clockwise from `start` (radians, 0 at the top),
    /// with round ends.
    Arc {
        rect: Rect,
        color: Color,
        thickness: f32,
        start: f32,
        sweep: f32,
    },
    /// A soft shadow of a rounded rectangle, blurred over `blur` pixels.
    Shadow {
        rect: Rect,
        corner_radius: f32,
        blur: f32,
        color: Color,
    },
    /// Text whose box starts at (`x`, `y`) and is `width` wide. Lines that do
    /// not fit the width wrap, and `align` places them within it.
    Text {
        x: f32,
        y: f32,
        width: f32,
        content: String,
        color: Color,
        font_size: f32,
        line_height: f32,
        /// From 100 (thin) to 900 (black); 400 is normal.
        font_weight: u16,
        font_family: FontFamily,
        align: TextAlign,
        max_lines: Option<usize>,
    },
    /// Like `Text`, with runs of different color, weight and family. The size
    /// and line height are shared.
    RichText {
        x: f32,
        y: f32,
        width: f32,
        spans: Vec<StyledSpan>,
        font_size: f32,
        line_height: f32,
        align: TextAlign,
        max_lines: Option<usize>,
    },
    /// A picture stretched over `rect`, with rounded corners.
    Image {
        rect: Rect,
        image: ImageData,
        corner_radius: f32,
        /// From 0 (invisible) to 1 (as is).
        opacity: f32,
    },
    /// Draws everything until the matching `PopLayer` as one picture, then puts
    /// it on what is below turned `rotation` radians clockwise and scaled by
    /// `scale` about the center of `rect`, and faded to `alpha`.
    PushLayer {
        rect: Rect,
        alpha: f32,
        rotation: f32,
        scale: f32,
    },
    PopLayer,
    /// A straight line from `from` to `to`, `width` thick, with round ends.
    Line {
        from: (f32, f32),
        to: (f32, f32),
        width: f32,
        color: Color,
    },
    /// A vector outline stretched over `rect` and filled with `color`.
    Path {
        path: Path,
        rect: Rect,
        color: Color,
    },
    /// Limits everything drawn until the matching `PopClip` to `rect`.
    PushClip {
        rect: Rect,
    },
    PopClip,
}

impl DrawCommand {
    /// This command moved by (`dx`, `dy`).
    pub fn translated(mut self, dx: f32, dy: f32) -> Self {
        let shift = |rect: &mut Rect| {
            rect.origin.x += dx;
            rect.origin.y += dy;
        };
        match &mut self {
            Self::Rect { rect, .. }
            | Self::Gradient { rect, .. }
            | Self::Arc { rect, .. }
            | Self::Shadow { rect, .. }
            | Self::Image { rect, .. }
            | Self::Path { rect, .. }
            | Self::PushClip { rect }
            | Self::PushLayer { rect, .. } => shift(rect),
            Self::Text { x, y, .. } | Self::RichText { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            Self::Line { from, to, .. } => {
                from.0 += dx;
                from.1 += dy;
                to.0 += dx;
                to.1 += dy;
            }
            Self::PopClip | Self::PopLayer => {}
        }
        self
    }
}
