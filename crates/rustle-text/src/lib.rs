//! Text styles and the measuring interface layout uses.
//!
//! The renderer implements [`TextMeasurer`], so layout can size text without
//! depending on a font system.

use rustle_geometry::Size;
use rustle_graphics::Color;
pub use rustle_graphics::{FontFamily, StyledSpan, TextAlign};

/// How heavy the strokes of a font are, from 100 (thin) to 900 (black).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const NORMAL: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMI_BOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
}

/// How a piece of text is set, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub color: Color,
    pub font_size: f32,
    pub line_height: f32,
    pub font_weight: FontWeight,
    pub font_family: FontFamily,
    pub align: TextAlign,
    /// Lines beyond this are cut off and the last one ends in an ellipsis.
    pub max_lines: Option<usize>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            color: Color::rgb(235, 235, 240),
            font_size: 14.0,
            line_height: 20.0,
            font_weight: FontWeight::NORMAL,
            font_family: FontFamily::SansSerif,
            align: TextAlign::Start,
            max_lines: None,
        }
    }
}

impl TextStyle {
    pub fn color(self, color: Color) -> Self {
        Self { color, ..self }
    }

    /// Sets the font size and scales the line height with it.
    pub fn font_size(self, font_size: f32) -> Self {
        Self {
            font_size,
            line_height: font_size * Self::default().line_height / Self::default().font_size,
            ..self
        }
    }

    pub fn font_weight(self, font_weight: FontWeight) -> Self {
        Self {
            font_weight,
            ..self
        }
    }

    pub fn font_family(self, font_family: FontFamily) -> Self {
        Self {
            font_family,
            ..self
        }
    }

    /// How lines line up within the text's width. Only visible when the text is wider than its content.
    pub fn align(self, align: TextAlign) -> Self {
        Self { align, ..self }
    }

    /// Shows at most `lines` lines, ending the last with an ellipsis if there is more.
    pub fn max_lines(self, lines: usize) -> Self {
        Self {
            max_lines: Some(lines),
            ..self
        }
    }

    pub fn bold(self) -> Self {
        self.font_weight(FontWeight::BOLD)
    }
}

/// A run of text within rich text. Anything left unset comes from the text style around it.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpan {
    pub text: String,
    pub color: Option<Color>,
    pub font_weight: Option<FontWeight>,
    pub font_family: Option<FontFamily>,
}

impl TextSpan {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
            font_weight: None,
            font_family: None,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn font_weight(mut self, font_weight: FontWeight) -> Self {
        self.font_weight = Some(font_weight);
        self
    }

    pub fn bold(self) -> Self {
        self.font_weight(FontWeight::BOLD)
    }

    pub fn font_family(mut self, font_family: FontFamily) -> Self {
        self.font_family = Some(font_family);
        self
    }

    /// This span with what it leaves unset taken from `style`.
    pub fn resolve(&self, style: &TextStyle) -> StyledSpan {
        StyledSpan {
            text: self.text.clone(),
            color: self.color.unwrap_or(style.color),
            font_weight: self.font_weight.unwrap_or(style.font_weight).0,
            font_family: self.font_family.unwrap_or(style.font_family),
        }
    }
}

/// Measures text.
pub trait TextMeasurer {
    /// The size `content` takes up when set in `style`, wrapping lines that
    /// are wider than `max_width`. An infinite `max_width` never wraps.
    fn measure(&mut self, content: &str, style: &TextStyle, max_width: f32) -> Size;

    /// The size of `spans` set one after another, which share `style`'s size and line height.
    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> Size;
}
