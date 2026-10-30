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

/// Where lines of text break when they are wider than the space they have.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum TextWrap {
    /// Between words, and inside a word only if it is wider than a whole line.
    #[default]
    Word,
    /// Between any two characters.
    Glyph,
    /// Never: lines end only at line breaks and may be wider than the space they have.
    None,
}

/// Which family of fonts text is set in.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum FontFamily {
    #[default]
    SansSerif,
    Serif,
    Monospace,
    /// An installed font by name, falling back to the default if it is missing.
    /// Use [`FontFamily::named`] to choose a family by a name only known at run time.
    Named(&'static str),
}

impl FontFamily {
    /// An installed (or registered) font family by a name that is not a literal.
    /// The name is kept for the rest of the program, once per distinct name, so
    /// the family stays `Copy`.
    pub fn named(name: &str) -> Self {
        use std::collections::HashSet;
        use std::sync::{Mutex, OnceLock};
        static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
        let mut names = NAMES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(known) = names.get(name) {
            return Self::Named(known);
        }
        let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
        names.insert(leaked);
        Self::Named(leaked)
    }
}

/// A run of text in one style, within a line of rich text.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledSpan {
    pub text: String,
    pub color: Color,
    /// From 100 (thin) to 900 (black); 400 is normal.
    pub font_weight: u16,
    pub font_family: FontFamily,
    /// Slanted letterforms (the font's italic, or an oblique if it has none).
    pub italic: bool,
    /// A line under the run, drawn by layout in the run's color.
    pub underline: bool,
    /// A line through the run, drawn by layout in the run's color.
    pub strikethrough: bool,
    /// A fill behind the run, drawn by layout.
    pub background: Option<Color>,
    /// The run's size in logical pixels, if it differs from the text's.
    pub font_size: Option<f32>,
    /// Extra space after every character, in logical pixels.
    pub letter_spacing: f32,
    /// Whether the font's ligatures (such as `fi`, or `=>` in a programming font) are used.
    pub ligatures: bool,
}

impl StyledSpan {
    /// A run of `text` with only a color, weight and family; everything else plain.
    pub fn new(
        text: impl Into<String>,
        color: Color,
        font_weight: u16,
        font_family: FontFamily,
    ) -> Self {
        Self {
            text: text.into(),
            color,
            font_weight,
            font_family,
            italic: false,
            underline: false,
            strikethrough: false,
            background: None,
            font_size: None,
            letter_spacing: 0.0,
            ligatures: true,
        }
    }
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
        italic: bool,
        /// Extra space after every character, in logical pixels.
        letter_spacing: f32,
        ligatures: bool,
        wrap: TextWrap,
    },
    /// Like `Text`, with runs that differ in color, weight, family, slant, size and
    /// letter spacing. The line height is shared. Underlines, strikethroughs and
    /// backgrounds are not drawn by this command: layout emits rectangles for them.
    RichText {
        x: f32,
        y: f32,
        width: f32,
        spans: Vec<StyledSpan>,
        font_size: f32,
        line_height: f32,
        align: TextAlign,
        max_lines: Option<usize>,
        wrap: TextWrap,
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
    /// Like `PushClip`, with corners rounded by `radii` (top left, top right,
    /// bottom right, bottom left), the edge antialiased, and applying to
    /// everything drawn: rectangles, text, pictures and outlines alike. Ended
    /// by the same `PopClip` as a rectangular clip, and may be nested in
    /// either kind.
    PushRoundedClip {
        rect: Rect,
        radii: [f32; 4],
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
            | Self::PushRoundedClip { rect, .. }
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
