//! Text styles and the measuring interface layout uses.
//!
//! The renderer implements [`TextMeasurer`], so layout can size text without
//! depending on a font system.

use rustle_geometry::Size;

/// How a piece of text is set, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub font_size: f32,
    pub line_height: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 18.0,
            line_height: 24.0,
        }
    }
}

/// Measures single-line text.
pub trait TextMeasurer {
    /// The size `content` takes up when set in `style`.
    fn measure(&mut self, content: &str, style: &TextStyle) -> Size;
}
