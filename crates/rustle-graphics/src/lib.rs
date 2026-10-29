//! What a renderer draws: backend-independent drawing commands.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rustle_geometry::Rect;

/// An sRGB color with straight alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// A picture: RGBA pixels, cheap to clone, identified so renderers can cache its texture.
#[derive(Clone)]
pub struct ImageData {
    id: u64,
    width: u32,
    height: u32,
    /// Straight-alpha RGBA, 8 bits per channel, row by row from the top.
    pixels: Arc<[u8]>,
}

impl ImageData {
    /// A picture from raw RGBA pixels, or `None` if `pixels` is not `width * height * 4` bytes.
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        (pixels.len() == width as usize * height as usize * 4).then(|| Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            pixels: pixels.into(),
        })
    }

    /// Decodes a PNG or JPEG file's bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let decoded = image::load_from_memory(bytes)
            .map_err(|error| error.to_string())?
            .to_rgba8();
        let (width, height) = decoded.dimensions();
        Self::from_rgba(width, height, decoded.into_raw())
            .ok_or_else(|| "decoded image has the wrong size".to_string())
    }

    /// The picture as the bytes of a PNG file.
    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        let buffer = image::RgbaImage::from_raw(self.width, self.height, self.pixels.to_vec())
            .ok_or_else(|| "pixels do not match the size".to_string())?;
        let mut bytes = std::io::Cursor::new(Vec::new());
        buffer
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        Ok(bytes.into_inner())
    }

    /// Identifies this picture; clones share it.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

impl std::fmt::Debug for ImageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ImageData({}x{}, id {})",
            self.width, self.height, self.id
        )
    }
}

impl PartialEq for ImageData {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

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
