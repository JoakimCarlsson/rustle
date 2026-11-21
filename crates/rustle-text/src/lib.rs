//! Text styles, glyph-level layout and the interfaces layout and renderers use.
//!
//! The renderer implements [`TextMeasurer`], so layout can size text without
//! depending on a font system, and installs a text engine that [`layout_text`],
//! [`layout_spans`], [`font_families`] and [`register_font`] use. That is how a
//! composable or a `canvas` closure asks where lines break and where a caret goes.

mod annotated;
mod engine;
mod kept;
mod layout;
mod wrap;

use rustle_geometry::Size;
use rustle_graphics::Color;
pub use rustle_graphics::{FontFamily, StyledSpan, TextAlign, TextWrap};

pub use annotated::{
    AnnotatedRange, AnnotatedString, AnnotatedStringBuilder, AnnotationStyle, LinkAnnotation,
    LinkInteractionListener, ParagraphStyle, Placeholder, PlaceholderVerticalAlign, SpanStyle,
    TextLinkStyles, TextUnit, build_annotated_string,
};
pub use engine::{
    TextEngine, font_epoch, font_families, install_text_engine, layout_spans, layout_styled,
    layout_text, monospace_font_families, register_font, take_pending_fonts,
};
pub use layout::{Affinity, Caret, Cluster, LineLayout, TextLayout};
pub use wrap::{WrappedLine, ellipsize, wrap_at_spaces};

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
    /// Slanted letterforms.
    pub italic: bool,
    /// A line under all of the text.
    pub underline: bool,
    /// A line through all of the text.
    pub strikethrough: bool,
    /// Extra space after every character, in logical pixels.
    pub letter_spacing: f32,
    /// Whether the font's ligatures are used. On by default.
    pub ligatures: bool,
    /// Where lines break when they are too wide.
    pub wrap: TextWrap,
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
            italic: false,
            underline: false,
            strikethrough: false,
            letter_spacing: 0.0,
            ligatures: true,
            wrap: TextWrap::Word,
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

    /// Sets the height of a line in logical pixels, independent of the font size.
    pub fn line_height(self, line_height: f32) -> Self {
        Self {
            line_height,
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

    /// Slanted letterforms.
    pub fn italic(self) -> Self {
        Self {
            italic: true,
            ..self
        }
    }

    /// Underlines all of the text.
    pub fn underline(self) -> Self {
        Self {
            underline: true,
            ..self
        }
    }

    /// Strikes through all of the text.
    pub fn strikethrough(self) -> Self {
        Self {
            strikethrough: true,
            ..self
        }
    }

    /// Extra space after every character, in logical pixels.
    pub fn letter_spacing(self, letter_spacing: f32) -> Self {
        Self {
            letter_spacing,
            ..self
        }
    }

    /// Turns the font's ligatures on or off. Turn them off in code, where `=>` should stay two characters.
    pub fn ligatures(self, ligatures: bool) -> Self {
        Self { ligatures, ..self }
    }

    /// Where lines break when they are wider than the space they have.
    pub fn wrap(self, wrap: TextWrap) -> Self {
        Self { wrap, ..self }
    }

    /// The size a span without a size of its own is set in, and the factor line height has to it.
    pub fn line_height_ratio(&self) -> f32 {
        if self.font_size > 0.0 {
            self.line_height / self.font_size
        } else {
            1.0
        }
    }
}

/// A run of text within rich text. Anything left unset comes from the text style around it.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpan {
    pub text: String,
    pub color: Option<Color>,
    pub font_weight: Option<FontWeight>,
    pub font_family: Option<FontFamily>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
    /// A fill behind the run. Unlike the others it has no counterpart in [`TextStyle`].
    pub background: Option<Color>,
    pub font_size: Option<f32>,
    pub letter_spacing: Option<f32>,
    pub ligatures: Option<bool>,
}

impl TextSpan {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
            font_weight: None,
            font_family: None,
            italic: None,
            underline: None,
            strikethrough: None,
            background: None,
            font_size: None,
            letter_spacing: None,
            ligatures: None,
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

    /// Slanted letterforms.
    pub fn italic(mut self) -> Self {
        self.italic = Some(true);
        self
    }

    /// A line under the run, in the run's color.
    pub fn underline(mut self) -> Self {
        self.underline = Some(true);
        self
    }

    /// A line through the run, in the run's color.
    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = Some(true);
        self
    }

    /// A fill behind the run, as tall as its line.
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Sets the run in `font_size` logical pixels instead of the text's size. The
    /// line height grows with it if it is the tallest run on its line.
    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = Some(font_size);
        self
    }

    /// Extra space after every character of the run, in logical pixels.
    pub fn letter_spacing(mut self, letter_spacing: f32) -> Self {
        self.letter_spacing = Some(letter_spacing);
        self
    }

    /// Turns the font's ligatures on or off for the run.
    pub fn ligatures(mut self, ligatures: bool) -> Self {
        self.ligatures = Some(ligatures);
        self
    }

    /// This span with what it leaves unset taken from `style`.
    pub fn resolve(&self, style: &TextStyle) -> StyledSpan {
        StyledSpan {
            text: self.text.clone(),
            color: self.color.unwrap_or(style.color),
            font_weight: self.font_weight.unwrap_or(style.font_weight).0,
            font_family: self.font_family.unwrap_or(style.font_family),
            italic: self.italic.unwrap_or(style.italic),
            underline: self.underline.unwrap_or(style.underline),
            strikethrough: self.strikethrough.unwrap_or(style.strikethrough),
            background: self.background,
            font_size: self.font_size,
            letter_spacing: self.letter_spacing.unwrap_or(style.letter_spacing),
            ligatures: self.ligatures.unwrap_or(style.ligatures),
        }
    }
}

/// Measures text.
pub trait TextMeasurer {
    /// The size `content` takes up when set in `style`, wrapping lines that
    /// are wider than `max_width`. An infinite `max_width` never wraps.
    fn measure(&mut self, content: &str, style: &TextStyle, max_width: f32) -> Size;

    /// The size of `spans` set one after another, which share `style`'s line height.
    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> Size;

    /// `spans` shaped and broken into lines, with the position of every cluster.
    /// An infinite `max_width` never wraps.
    fn layout(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> TextLayout;
}
