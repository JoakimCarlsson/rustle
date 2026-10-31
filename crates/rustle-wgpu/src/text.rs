//! Text: shaping with glyphon, drawing it, and measuring it for layout.

use std::cell::RefCell;
use std::rc::Rc;

use glyphon::cosmic_text::{
    Align, Ellipsize, EllipsizeHeightLimit, FeatureTag, FontFeatures, Hinting, Style,
};
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, TextArea, TextBounds, TextRenderer,
    Weight, Wrap,
};
use rustle_geometry::Size;
use rustle_graphics::{FontFamily, StyledSpan, TextAlign, TextWrap};
use rustle_text::{TextLayout, TextMeasurer, TextStyle};

use crate::engine::layout_spans;

use std::hash::{Hash, Hasher};

use crate::{Renderer, Result};

impl Renderer {
    pub(super) fn prepare_text(
        &mut self,
        index: usize,
        texts: &[Shaped],
        scale_factor: f32,
        target: (u32, u32),
    ) -> Result<()> {
        while self.text_renderers.len() <= index {
            self.text_renderers.push(TextRenderer::new(
                &mut self.atlas,
                &self.device,
                wgpu::MultisampleState::default(),
                None,
            ));
        }
        let bounds = TextBounds {
            left: 0,
            top: 0,
            right: target.0 as i32,
            bottom: target.1 as i32,
        };
        let areas = texts.iter().map(|text| TextArea {
            buffer: &text.buffer,
            left: (text.position.0 * scale_factor).round(),
            top: (text.position.1 * scale_factor).round(),
            scale: scale_factor,
            bounds,
            default_color: text.color,
            custom_glyphs: &[],
        });
        self.text_renderers[index].prepare(
            &self.device,
            &self.queue,
            &mut self.font_system.borrow_mut(),
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        )?;
        Ok(())
    }
}

pub(super) struct Shaped {
    pub(super) position: (f32, f32),
    pub(super) color: Color,
    pub(super) buffer: Rc<Buffer>,
}
impl TextMeasurer for Renderer {
    fn measure(&mut self, content: &str, style: &TextStyle, max_width: f32) -> Size {
        // Layout asks for the same text again every frame; shaping is the most
        // expensive part of it, so sizes are kept until unused for a while.
        let key = measure_key(|hasher| {
            content.hash(hasher);
            // Every field of the style can change the size, so all of them count.
            format!("{style:?}").hash(hasher);
            max_width.to_bits().hash(hasher);
            // Registering a font can change every size.
            self.font_generation.get().hash(hasher);
        });
        if let Some((size, used)) = self.measured.get_mut(&key) {
            *used = self.frame;
            return *size;
        }
        let size = measure_plain(
            &mut self.font_system.borrow_mut(),
            content,
            style,
            max_width,
        );
        self.measured.insert(key, (size, self.frame));
        size
    }

    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> Size {
        let key = measure_key(|hasher| {
            // Every field of a span can change its size, so all of them count.
            format!("{spans:?}").hash(hasher);
            format!("{style:?}").hash(hasher);
            max_width.to_bits().hash(hasher);
            self.font_generation.get().hash(hasher);
            1_u8.hash(hasher);
        });
        if let Some((size, used)) = self.measured.get_mut(&key) {
            *used = self.frame;
            return *size;
        }
        let size = measure_rich(&mut self.font_system.borrow_mut(), spans, style, max_width);
        self.measured.insert(key, (size, self.frame));
        size
    }

    fn layout(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> TextLayout {
        layout_spans(
            &mut self.font_system.borrow_mut(),
            spans,
            style,
            max_width.is_finite().then_some(max_width),
        )
    }
}

/// A hash identifying a text measurement, from what `fill` feeds the hasher.
fn measure_key(fill: impl FnOnce(&mut std::collections::hash_map::DefaultHasher)) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    fill(&mut hasher);
    hasher.finish()
}

/// The size of `content` set in `style`, wrapped at `max_width` if it is finite.
fn measure_plain(
    font_system: &mut FontSystem,
    content: &str,
    style: &TextStyle,
    max_width: f32,
) -> Size {
    let buffer = shape(
        font_system,
        Setting {
            content: Content::Plain {
                text: content,
                font_weight: legible_weight(style.font_weight.0, style.color),
                font_family: style.font_family,
                italic: style.italic,
                letter_spacing: style.letter_spacing,
                ligatures: style.ligatures,
            },
            font_size: style.font_size,
            line_height: style.line_height,
            align: TextAlign::Start,
            max_lines: style.max_lines,
            wrap: style.wrap,
        },
        max_width.is_finite().then_some(max_width),
    );
    // A trailing line break leaves an empty last line the shaper does not report.
    let breaks = content.matches('\n').count() as u32 + 1;
    size_of_buffer(&buffer, style.line_height, breaks)
}

/// The size of `spans` set in `style`, wrapped at `max_width` if it is finite.
fn measure_rich(
    font_system: &mut FontSystem,
    spans: &[StyledSpan],
    style: &TextStyle,
    max_width: f32,
) -> Size {
    let buffer = shape(
        font_system,
        Setting {
            content: Content::Rich(spans),
            font_size: style.font_size,
            line_height: style.line_height,
            align: TextAlign::Start,
            max_lines: style.max_lines,
            wrap: style.wrap,
        },
        max_width.is_finite().then_some(max_width),
    );
    size_of_buffer(&buffer, style.line_height, 0)
}

fn size_of_buffer(buffer: &Buffer, line_height: f32, at_least: u32) -> Size {
    // Lines are as tall as their tallest run, which is more than `line_height` when a span has a larger size.
    let (width, height, lines) =
        buffer
            .layout_runs()
            .fold((0.0_f32, 0.0_f32, 0_u32), |(width, height, lines), run| {
                (
                    width.max(run.line_w),
                    height + run.line_height.max(line_height),
                    lines + 1,
                )
            });
    let missing = at_least.max(1).saturating_sub(lines);
    Size::new(width, height + missing as f32 * line_height)
}

/// Measures and lays out text with the fonts of this thread, without a GPU.
///
/// What a host uses to lay an interface out on a thread that has no renderer, when the
/// renderer that draws it lives on another one. It shares its fonts with every
/// [`Renderer`] made on the thread.
pub struct TextMetrics {
    font_system: Rc<RefCell<FontSystem>>,
}

impl TextMetrics {
    /// A measurer using the fonts of this thread, which are loaded the first time one is needed.
    pub fn new() -> Self {
        let (font_system, _) = crate::device::shared_fonts();
        Self { font_system }
    }
}

impl Default for TextMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl TextMeasurer for TextMetrics {
    fn measure(&mut self, content: &str, style: &TextStyle, max_width: f32) -> Size {
        measure_plain(
            &mut self.font_system.borrow_mut(),
            content,
            style,
            max_width,
        )
    }

    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> Size {
        measure_rich(&mut self.font_system.borrow_mut(), spans, style, max_width)
    }

    fn layout(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> TextLayout {
        layout_spans(
            &mut self.font_system.borrow_mut(),
            spans,
            style,
            max_width.is_finite().then_some(max_width),
        )
    }
}

/// How a piece of text is set.
pub(super) struct Setting<'a> {
    pub(super) content: Content<'a>,
    pub(super) font_size: f32,
    pub(super) line_height: f32,
    pub(super) align: TextAlign,
    pub(super) max_lines: Option<usize>,
    pub(super) wrap: TextWrap,
}

/// What a piece of text says.
pub(super) enum Content<'a> {
    /// One run in one style.
    Plain {
        text: &'a str,
        font_weight: u16,
        font_family: FontFamily,
        italic: bool,
        letter_spacing: f32,
        ligatures: bool,
    },
    /// Runs in their own styles.
    Rich(&'a [StyledSpan]),
}

/// Whether text in `color` sits on a light surface.
fn is_dark(color: rustle_graphics::Color) -> bool {
    let luminance =
        0.2126 * f32::from(color.r) + 0.7152 * f32::from(color.g) + 0.0722 * f32::from(color.b);
    luminance < 110.0
}

/// Dark text is drawn pure black, the strongest contrast antialiasing allows.
pub(super) fn legible_color(color: rustle_graphics::Color) -> Color {
    if is_dark(color) {
        Color::rgba(0, 0, 0, color.a)
    } else {
        Color::rgba(color.r, color.g, color.b, color.a)
    }
}

/// Dark text on a light surface blends thinner than light text on a dark one,
/// so it is set one step heavier to stay as readable.
pub(super) fn legible_weight(weight: u16, color: rustle_graphics::Color) -> u16 {
    if is_dark(color) && weight < 600 {
        600
    } else {
        weight
    }
}

fn family(family: FontFamily) -> Family<'static> {
    match family {
        FontFamily::SansSerif => Family::SansSerif,
        FontFamily::Serif => Family::Serif,
        FontFamily::Monospace => Family::Monospace,
        FontFamily::Named(name) => Family::Name(name),
    }
}

fn font_features(ligatures: bool) -> FontFeatures {
    let mut features = FontFeatures::new();
    if !ligatures {
        features
            .disable(FeatureTag::STANDARD_LIGATURES)
            .disable(FeatureTag::CONTEXTUAL_LIGATURES)
            .disable(FeatureTag::CONTEXTUAL_ALTERNATES);
    }
    features
}

/// The attributes of a run: family, weight, slant, letter spacing, ligatures and,
/// if it has a size of its own, the metrics that size gives.
fn attributes(
    font_family: FontFamily,
    font_weight: u16,
    italic: bool,
    letter_spacing: f32,
    ligatures: bool,
    size: f32,
    own_size: Option<(f32, f32)>,
) -> Attrs<'static> {
    let mut attrs = Attrs::new()
        .family(family(font_family))
        .weight(Weight(font_weight))
        .style(if italic { Style::Italic } else { Style::Normal });
    if letter_spacing != 0.0 && size > 0.0 {
        attrs = attrs.letter_spacing(letter_spacing / size);
    }
    if !ligatures {
        attrs = attrs.font_features(font_features(false));
    }
    if let Some((size, line_height)) = own_size {
        attrs = attrs.metrics(Metrics::new(size, line_height));
    }
    attrs
}

/// Shapes text in logical pixels, wrapping at `width` if there is one.
pub(super) fn shape(
    font_system: &mut FontSystem,
    setting: Setting<'_>,
    width: Option<f32>,
) -> Buffer {
    let mut buffer = Buffer::new(
        font_system,
        Metrics::new(setting.font_size, setting.line_height),
    );
    // Advances are not snapped to whole pixels: snapping makes the glyphs sit further apart
    // than the line widths that wrapping and measuring use, so wrapped text would overflow
    // its box, and it is wrong in logical pixels on a scaled display.
    buffer.set_hinting(Hinting::Disabled);
    buffer.set_size(width, None);
    buffer.set_wrap(match setting.wrap {
        TextWrap::Word => Wrap::WordOrGlyph,
        TextWrap::Glyph => Wrap::Glyph,
        TextWrap::None => Wrap::None,
    });
    buffer.set_ellipsize(match setting.max_lines {
        Some(lines) => Ellipsize::End(EllipsizeHeightLimit::Lines(lines)),
        None => Ellipsize::None,
    });
    let alignment = match setting.align {
        TextAlign::Start => None,
        TextAlign::Left => Some(Align::Left),
        TextAlign::Center => Some(Align::Center),
        TextAlign::Right => Some(Align::Right),
    };
    let ratio = if setting.font_size > 0.0 {
        setting.line_height / setting.font_size
    } else {
        1.0
    };
    match setting.content {
        Content::Plain {
            text,
            font_weight,
            font_family,
            italic,
            letter_spacing,
            ligatures,
        } => buffer.set_text(
            text,
            &attributes(
                font_family,
                font_weight,
                italic,
                letter_spacing,
                ligatures,
                setting.font_size,
                None,
            ),
            Shaping::Advanced,
            alignment,
        ),
        Content::Rich(spans) => buffer.set_rich_text(
            spans.iter().enumerate().map(|(index, span)| {
                let size = span.font_size.unwrap_or(setting.font_size);
                (
                    span.text.as_str(),
                    attributes(
                        span.font_family,
                        legible_weight(span.font_weight, span.color),
                        span.italic,
                        span.letter_spacing,
                        span.ligatures,
                        size,
                        span.font_size.map(|size| (size, size * ratio)),
                    )
                    .metadata(index)
                    .color(Color::rgba(
                        span.color.r,
                        span.color.g,
                        span.color.b,
                        span.color.a,
                    )),
                )
            }),
            &Attrs::new(),
            Shaping::Advanced,
            alignment,
        ),
    }
    buffer.shape_until_scroll(font_system, false);
    buffer
}
