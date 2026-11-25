//! Text: shaping with glyphon, drawing it, and measuring it for layout.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glyphon::cosmic_text::{
    Align, Ellipsize, EllipsizeHeightLimit, FeatureTag, FontFeatures, Hinting, Style,
};
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Resolution, Shaping, TextArea, TextBounds,
    TextRenderer, Viewport, Weight, Wrap,
};
use rustle_geometry::Size;
use rustle_graphics::{FontFamily, StyledSpan, TextAlign, TextWrap};
use rustle_text::{TextLayout, TextMeasurer, TextStyle};

use crate::composite::Canvas;

use std::hash::{Hash, Hasher};

use crate::{Renderer, Result};

impl Renderer {
    pub(super) fn prepare_text(
        &mut self,
        index: usize,
        texts: &[Shaped],
        scale_factor: f32,
        canvas: Canvas,
    ) -> Result<()> {
        self.text_viewport(canvas.size);
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
            right: canvas.size.0 as i32,
            bottom: canvas.size.1 as i32,
        };
        let (shift_x, shift_y) = (canvas.origin.0 as f32, canvas.origin.1 as f32);
        let areas = texts.iter().map(|text| TextArea {
            buffer: &text.buffer,
            left: (text.position.0 * scale_factor).round() - shift_x,
            top: (text.position.1 * scale_factor).round() - shift_y,
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
            &self.text_viewports[&canvas.size],
            areas,
            &mut self.swash_cache,
        )?;
        Ok(())
    }

    /// Makes sure glyphon has a viewport for textures of `size`, which text drawn into
    /// them is positioned against.
    pub(super) fn text_viewport(&mut self, size: (u32, u32)) {
        let viewport = self
            .text_viewports
            .entry(size)
            .or_insert_with(|| Viewport::new(&self.device, &self.text_cache));
        viewport.update(
            &self.queue,
            Resolution {
                width: size.0,
                height: size.1,
            },
        );
    }
}

pub(super) struct Shaped {
    pub(super) position: (f32, f32),
    pub(super) color: Color,
    pub(super) buffer: Rc<Buffer>,
}
impl TextMeasurer for Renderer {
    /// Measures plain text, reusing its size and refreshing its cache frame on a hit.
    fn measure(&mut self, content: &str, style: &TextStyle, max_width: f32) -> Size {
        let key = plain_measure_key(content, style, max_width, self.font_generation.get());
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

    /// Measures rich text, reusing its size and refreshing its cache frame on a hit.
    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> Size {
        let key = rich_measure_key(spans, style, max_width, self.font_generation.get());
        if let Some((size, used)) = self.measured.get_mut(&key) {
            *used = self.frame;
            return *size;
        }
        let size = measure_rich(&mut self.font_system.borrow_mut(), spans, style, max_width);
        self.measured.insert(key, (size, self.frame));
        size
    }

    /// Lays out rich text through the thread's text engine, which keeps layouts of text laid
    /// out before with the same runs, style, width and fonts.
    fn layout(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> TextLayout {
        rustle_text::layout_styled(spans, style, max_width.is_finite().then_some(max_width))
    }
}

/// A hash identifying a text measurement, from what `fill` feeds the hasher.
fn measure_key(fill: impl FnOnce(&mut std::collections::hash_map::DefaultHasher)) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    fill(&mut hasher);
    hasher.finish()
}

/// Identifies plain text, its complete style, width bits and registered-font generation.
fn plain_measure_key(content: &str, style: &TextStyle, max_width: f32, generation: u64) -> u64 {
    measure_key(|hasher| {
        0_u8.hash(hasher);
        content.hash(hasher);
        hash_style(style, hasher);
        max_width.to_bits().hash(hasher);
        generation.hash(hasher);
    })
}

/// Identifies ordered rich runs, their complete styles, width bits and font generation.
fn rich_measure_key(
    spans: &[StyledSpan],
    style: &TextStyle,
    max_width: f32,
    generation: u64,
) -> u64 {
    measure_key(|hasher| {
        1_u8.hash(hasher);
        spans.len().hash(hasher);
        for span in spans {
            hash_span(span, hasher);
        }
        hash_style(style, hasher);
        max_width.to_bits().hash(hasher);
        generation.hash(hasher);
    })
}

/// Feeds every style field into `hasher` without allocating, preserving float bits.
fn hash_style(style: &TextStyle, hasher: &mut impl Hasher) {
    let TextStyle {
        color,
        font_size,
        line_height,
        font_weight,
        font_family,
        align,
        max_lines,
        italic,
        underline,
        strikethrough,
        letter_spacing,
        ligatures,
        wrap,
    } = style;
    let rustle_text::FontWeight(weight) = font_weight;
    hash_color(*color, hasher);
    font_size.to_bits().hash(hasher);
    line_height.to_bits().hash(hasher);
    weight.hash(hasher);
    hash_family(*font_family, hasher);
    std::mem::discriminant(align).hash(hasher);
    max_lines.hash(hasher);
    italic.hash(hasher);
    underline.hash(hasher);
    strikethrough.hash(hasher);
    letter_spacing.to_bits().hash(hasher);
    ligatures.hash(hasher);
    std::mem::discriminant(wrap).hash(hasher);
}

/// Feeds a run's text boundary and every style field into `hasher` without allocating.
fn hash_span(span: &StyledSpan, hasher: &mut impl Hasher) {
    let StyledSpan {
        text,
        color,
        font_weight,
        font_family,
        italic,
        underline,
        strikethrough,
        background,
        font_size,
        letter_spacing,
        ligatures,
    } = span;
    text.hash(hasher);
    hash_color(*color, hasher);
    font_weight.hash(hasher);
    hash_family(*font_family, hasher);
    italic.hash(hasher);
    underline.hash(hasher);
    strikethrough.hash(hasher);
    std::mem::discriminant(background).hash(hasher);
    if let Some(color) = background {
        hash_color(*color, hasher);
    }
    font_size.map(f32::to_bits).hash(hasher);
    letter_spacing.to_bits().hash(hasher);
    ligatures.hash(hasher);
}

/// Feeds all four color channels into `hasher`, including alpha.
fn hash_color(color: rustle_graphics::Color, hasher: &mut impl Hasher) {
    let rustle_graphics::Color { r, g, b, a } = color;
    (r, g, b, a).hash(hasher);
}

/// Feeds the family variant and any named-family text into `hasher`.
fn hash_family(family: FontFamily, hasher: &mut impl Hasher) {
    std::mem::discriminant(&family).hash(hasher);
    if let FontFamily::Named(name) = family {
        name.hash(hasher);
    }
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
    let breaks =
        (content.matches('\n').count() + 1).min(style.max_lines.unwrap_or(usize::MAX)) as u32;
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
    /// Sizes of text measured for layout, keyed by a hash of what was measured and the font
    /// epoch, as a renderer keeps them; dropped all at once when [`METRICS_KEPT`] are held.
    measured: HashMap<u64, Size>,
}

/// How many measured sizes a [`TextMetrics`] keeps before it starts over.
const METRICS_KEPT: usize = 16_384;

impl TextMetrics {
    /// A measurer using the fonts of this thread, which are loaded the first time one is needed.
    pub fn new() -> Self {
        let (font_system, _) = crate::device::shared_fonts();
        Self {
            font_system,
            measured: HashMap::new(),
        }
    }

    /// The size kept under `key`, or the one `measure` finds, which is then kept.
    fn kept(&mut self, key: u64, measure: impl FnOnce(&mut FontSystem) -> Size) -> Size {
        if let Some(size) = self.measured.get(&key) {
            return *size;
        }
        let size = measure(&mut self.font_system.borrow_mut());
        if self.measured.len() >= METRICS_KEPT {
            self.measured.clear();
        }
        self.measured.insert(key, size);
        size
    }
}

impl Default for TextMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl TextMeasurer for TextMetrics {
    /// Measures plain text, reusing the size measured before for the same text, style, width
    /// and fonts.
    fn measure(&mut self, content: &str, style: &TextStyle, max_width: f32) -> Size {
        let key = plain_measure_key(content, style, max_width, rustle_text::font_epoch());
        self.kept(key, |fonts| measure_plain(fonts, content, style, max_width))
    }

    /// Measures rich text, reusing the size measured before for the same runs, style, width
    /// and fonts.
    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> Size {
        let key = rich_measure_key(spans, style, max_width, rustle_text::font_epoch());
        self.kept(key, |fonts| measure_rich(fonts, spans, style, max_width))
    }

    /// Lays out rich text through the thread's text engine, which keeps layouts of text laid
    /// out before with the same runs, style, width and fonts.
    fn layout(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> TextLayout {
        rustle_text::layout_styled(spans, style, max_width.is_finite().then_some(max_width))
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

/// Whether dark text is drawn black and heavier; see [`set_legible_text`].
static LEGIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Sets whether dark text is drawn pure black and at least semibold, which is on by default:
/// dark text is taken to sit on a light surface, where it blends thinner than light text on a
/// dark one. An application whose palette sets grey text on dark surfaces turns it off, so text
/// is drawn in exactly its color and weight. It applies to every renderer in the program and to
/// measuring, so set it before the first frame is laid out.
pub fn set_legible_text(on: bool) {
    LEGIBLE.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether text in `color` sits on a light surface, and so is drawn legibly.
fn is_dark(color: rustle_graphics::Color) -> bool {
    if !LEGIBLE.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
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

/// The weight of the face of `font_family` a request for `weight` settles on, so a named
/// family without that exact weight is drawn in its nearest face rather than in another
/// family that has it; generic families keep `weight`.
///
/// Each answer is kept by the family, the weight and how many faces the font system
/// holds, so text set again does not search the faces for every run, and a font loaded
/// later is searched for.
fn available_weight(font_system: &FontSystem, font_family: FontFamily, weight: u16) -> u16 {
    let FontFamily::Named(name) = font_family else {
        return weight;
    };
    let faces = font_system.db().len();
    AVAILABLE_WEIGHTS.with(|kept| {
        let mut kept = kept.borrow_mut();
        if kept.0 != faces {
            *kept = (faces, HashMap::new());
        }
        *kept.1.entry((name, weight)).or_insert_with(|| {
            let query = glyphon::cosmic_text::fontdb::Query {
                families: &[family(font_family)],
                weight: Weight(weight),
                ..Default::default()
            };
            font_system
                .db()
                .query(&query)
                .and_then(|id| font_system.db().face(id))
                .map_or(weight, |face| face.weight.0)
        })
    })
}

/// The weights of named families found by [`available_weight`], by family and weight
/// asked for, with how many faces the font system held when they were found.
type AvailableWeights = (usize, HashMap<(&'static str, u16), u16>);

thread_local! {
    /// What [`available_weight`] found, kept while the font system holds as many faces.
    static AVAILABLE_WEIGHTS: RefCell<AvailableWeights> = RefCell::new((0, HashMap::new()));
    /// How many texts were set since the shaped words were last trimmed.
    static SHAPED_SINCE_TRIM: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// How many faces the font system held, and the font epoch, when its shaped words were
    /// kept: a font loaded since may shape them otherwise.
    static SHAPED_WITH: std::cell::Cell<(usize, u64)> = const { std::cell::Cell::new((0, 0)) };
}

/// How many texts are set between trims of the shaped words the font system keeps.
const SHAPES_PER_TRIM: u32 = 1024;

/// How many trims a shaped word survives unused.
const TRIMS_KEPT: u64 = 4;

/// Counts one text set, and every [`SHAPES_PER_TRIM`] lets go of the shaped words the
/// font system keeps that no text set in the last [`TRIMS_KEPT`] stretches used: the words
/// are kept so that text set again, such as a paragraph growing as it streams in, shapes
/// only the words it did not have before. All of them are let go once the fonts change.
/// Called before a text is shaped.
fn trim_shaped_words(font_system: &mut FontSystem) {
    let fonts = (font_system.db().len(), rustle_text::font_epoch());
    if SHAPED_WITH.with(|kept| kept.replace(fonts)) != fonts {
        font_system.shape_run_cache = Default::default();
    }
    let due = SHAPED_SINCE_TRIM.with(|count| {
        count.set(count.get() + 1);
        count.get() >= SHAPES_PER_TRIM
    });
    if due {
        SHAPED_SINCE_TRIM.with(|count| count.set(0));
        font_system.shape_run_cache.trim(TRIMS_KEPT);
    }
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

/// Shapes text in logical pixels, wrapping at `width` if there is one. Text limited to a
/// number of lines keeps at most that many of its own lines, so a line break in it never
/// shows more lines than the limit.
pub(super) fn shape(
    font_system: &mut FontSystem,
    setting: Setting<'_>,
    width: Option<f32>,
) -> Buffer {
    trim_shaped_words(font_system);
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
                available_weight(font_system, font_family, font_weight),
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
                        available_weight(
                            font_system,
                            span.font_family,
                            legible_weight(span.font_weight, span.color),
                        ),
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
    if let Some(lines) = setting.max_lines {
        buffer.lines.truncate(lines.max(1));
    }
    buffer.shape_until_scroll(font_system, false);
    buffer
}

/// Pure cache-key coverage; no renderer or font system is constructed.
#[cfg(test)]
mod tests {
    use super::*;
    use rustle_graphics::Color as TextColor;
    use rustle_text::FontWeight;

    /// Supplies a plain run whose optional styling is initially absent.
    fn span(text: &str) -> StyledSpan {
        StyledSpan::new(
            text,
            TextColor::rgb(235, 235, 240),
            400,
            FontFamily::SansSerif,
        )
    }

    /// Identical inputs retain their keys, while content and cache domains differ.
    #[test]
    fn keys_preserve_identity_and_separate_domains() {
        let style = TextStyle::default();
        let spans = [span("text")];
        let plain = plain_measure_key("text", &style, 100.0, 7);
        let rich = rich_measure_key(&spans, &style, 100.0, 7);
        assert_eq!(plain, plain_measure_key("text", &style, 100.0, 7));
        assert_eq!(rich, rich_measure_key(&spans.clone(), &style, 100.0, 7));
        assert_ne!(plain, rich);
        assert_ne!(plain, plain_measure_key("other", &style, 100.0, 7));
    }

    /// Text limited to one line is one line tall and no wider than its first line, though
    /// it holds a line break, plain or in runs.
    #[test]
    fn a_line_limit_holds_across_line_breaks() {
        let mut metrics = TextMetrics::new();
        let one = TextStyle::default().max_lines(1);
        let first = metrics.measure("short", &one, f32::INFINITY);
        let broken = metrics.measure("short\na much longer second line", &one, f32::INFINITY);
        assert_eq!(broken, first);
        let rich = metrics.measure_spans(&[span("short")], &one, f32::INFINITY);
        let rich_broken = metrics.measure_spans(
            &[span("short\na much longer second line")],
            &one,
            f32::INFINITY,
        );
        assert_eq!(rich_broken, rich);
        let two = metrics.measure("a\nb\nc", &TextStyle::default().max_lines(2), f32::INFINITY);
        assert_eq!(two.height, first.height * 2.0);
    }

    /// Every base-style property contributes to both plain and rich keys.
    #[test]
    fn keys_include_every_style_field() {
        let base = TextStyle::default();
        let spans = [span("text")];
        let variants = [
            TextStyle {
                color: TextColor { r: 1, ..base.color },
                ..base
            },
            TextStyle {
                color: TextColor { g: 1, ..base.color },
                ..base
            },
            TextStyle {
                color: TextColor { b: 1, ..base.color },
                ..base
            },
            TextStyle {
                color: TextColor { a: 1, ..base.color },
                ..base
            },
            TextStyle {
                font_size: 15.0,
                ..base
            },
            TextStyle {
                line_height: 21.0,
                ..base
            },
            TextStyle {
                font_weight: FontWeight(500),
                ..base
            },
            TextStyle {
                font_family: FontFamily::Serif,
                ..base
            },
            TextStyle {
                font_family: FontFamily::Monospace,
                ..base
            },
            TextStyle {
                font_family: FontFamily::Named("SansSerif"),
                ..base
            },
            TextStyle {
                align: TextAlign::Left,
                ..base
            },
            TextStyle {
                align: TextAlign::Center,
                ..base
            },
            TextStyle {
                align: TextAlign::Right,
                ..base
            },
            TextStyle {
                max_lines: Some(0),
                ..base
            },
            TextStyle {
                max_lines: Some(1),
                ..base
            },
            TextStyle {
                italic: true,
                ..base
            },
            TextStyle {
                underline: true,
                ..base
            },
            TextStyle {
                strikethrough: true,
                ..base
            },
            TextStyle {
                letter_spacing: 1.0,
                ..base
            },
            TextStyle {
                letter_spacing: -0.0,
                ..base
            },
            TextStyle {
                ligatures: false,
                ..base
            },
            TextStyle {
                wrap: TextWrap::Glyph,
                ..base
            },
            TextStyle {
                wrap: TextWrap::None,
                ..base
            },
        ];
        let plain = plain_measure_key("text", &base, 100.0, 7);
        let rich = rich_measure_key(&spans, &base, 100.0, 7);
        for style in variants {
            assert_ne!(
                plain,
                plain_measure_key("text", &style, 100.0, 7),
                "{style:?}"
            );
            assert_ne!(
                rich,
                rich_measure_key(&spans, &style, 100.0, 7),
                "{style:?}"
            );
        }
        for family in [
            FontFamily::SansSerif,
            FontFamily::Serif,
            FontFamily::Monospace,
            FontFamily::Named("first"),
            FontFamily::Named("second"),
        ] {
            let key = measure_key(|hasher| hash_family(family, hasher));
            for other in [
                FontFamily::SansSerif,
                FontFamily::Serif,
                FontFamily::Monospace,
                FontFamily::Named("first"),
                FontFamily::Named("second"),
            ] {
                if family != other {
                    assert_ne!(key, measure_key(|hasher| hash_family(other, hasher)));
                }
            }
        }
    }

    /// Width is bitwise and a font registration invalidates both cache domains.
    #[test]
    fn keys_include_width_bits_and_font_generation() {
        let style = TextStyle::default();
        let spans = [span("text")];
        let widths = [
            0.0,
            -0.0,
            100.0,
            f32::INFINITY,
            f32::from_bits(0x7fc0_0001),
            f32::from_bits(0x7fc0_0002),
        ];
        for (index, width) in widths.iter().enumerate() {
            for other in &widths[index + 1..] {
                assert_ne!(
                    plain_measure_key("text", &style, *width, 7),
                    plain_measure_key("text", &style, *other, 7)
                );
                assert_ne!(
                    rich_measure_key(&spans, &style, *width, 7),
                    rich_measure_key(&spans, &style, *other, 7)
                );
            }
            assert_ne!(
                plain_measure_key("text", &style, *width, 7),
                plain_measure_key("text", &style, *width, 8)
            );
            assert_ne!(
                rich_measure_key(&spans, &style, *width, 7),
                rich_measure_key(&spans, &style, *width, 8)
            );
        }
    }

    /// Span text, order, count and text boundaries remain part of rich identity.
    #[test]
    fn rich_keys_include_sequence_structure() {
        let style = TextStyle::default();
        let variants = [
            vec![],
            vec![span("")],
            vec![span("ab")],
            vec![span("a"), span("b")],
            vec![span("b"), span("a")],
            vec![span("ab"), span("")],
            vec![span(""), span("ab")],
            vec![span("abc"), span("d")],
            vec![span("a"), span("bcd")],
        ];
        for (index, spans) in variants.iter().enumerate() {
            for other in &variants[index + 1..] {
                assert_ne!(
                    rich_measure_key(spans, &style, 100.0, 7),
                    rich_measure_key(other, &style, 100.0, 7)
                );
            }
        }
    }

    /// Every run property, including optional discriminants and payloads, is hashed.
    #[test]
    fn rich_keys_include_every_span_field() {
        let style = TextStyle::default();
        let base = span("text");
        let variants = [
            StyledSpan {
                text: "other".into(),
                ..base.clone()
            },
            StyledSpan {
                color: TextColor { r: 1, ..base.color },
                ..base.clone()
            },
            StyledSpan {
                color: TextColor { g: 1, ..base.color },
                ..base.clone()
            },
            StyledSpan {
                color: TextColor { b: 1, ..base.color },
                ..base.clone()
            },
            StyledSpan {
                color: TextColor { a: 1, ..base.color },
                ..base.clone()
            },
            StyledSpan {
                font_weight: 500,
                ..base.clone()
            },
            StyledSpan {
                font_family: FontFamily::Serif,
                ..base.clone()
            },
            StyledSpan {
                italic: true,
                ..base.clone()
            },
            StyledSpan {
                underline: true,
                ..base.clone()
            },
            StyledSpan {
                strikethrough: true,
                ..base.clone()
            },
            StyledSpan {
                background: Some(TextColor::TRANSPARENT),
                ..base.clone()
            },
            StyledSpan {
                background: Some(TextColor::rgba(1, 0, 0, 0)),
                ..base.clone()
            },
            StyledSpan {
                background: Some(TextColor::rgba(0, 1, 0, 0)),
                ..base.clone()
            },
            StyledSpan {
                background: Some(TextColor::rgba(0, 0, 1, 0)),
                ..base.clone()
            },
            StyledSpan {
                background: Some(TextColor::rgba(0, 0, 0, 1)),
                ..base.clone()
            },
            StyledSpan {
                font_size: Some(0.0),
                ..base.clone()
            },
            StyledSpan {
                font_size: Some(-0.0),
                ..base.clone()
            },
            StyledSpan {
                font_size: Some(15.0),
                ..base.clone()
            },
            StyledSpan {
                letter_spacing: 1.0,
                ..base.clone()
            },
            StyledSpan {
                letter_spacing: -0.0,
                ..base.clone()
            },
            StyledSpan {
                ligatures: false,
                ..base.clone()
            },
        ];
        let key = rich_measure_key(std::slice::from_ref(&base), &style, 100.0, 7);
        for (index, span) in variants.iter().enumerate() {
            let changed = rich_measure_key(std::slice::from_ref(span), &style, 100.0, 7);
            assert_ne!(key, changed, "{span:?}");
            for other in &variants[index + 1..] {
                assert_ne!(
                    changed,
                    rich_measure_key(std::slice::from_ref(other), &style, 100.0, 7)
                );
            }
        }
    }

    /// Style floats retain signed zeros and distinct NaN payloads in both domains.
    #[test]
    fn keys_preserve_style_float_bits() {
        let spans = [span("text")];
        for (value, other) in [
            (0.0, -0.0),
            (f32::from_bits(0x7fc0_0001), f32::from_bits(0x7fc0_0002)),
        ] {
            for (first, second) in [
                (
                    TextStyle {
                        font_size: value,
                        ..TextStyle::default()
                    },
                    TextStyle {
                        font_size: other,
                        ..TextStyle::default()
                    },
                ),
                (
                    TextStyle {
                        line_height: value,
                        ..TextStyle::default()
                    },
                    TextStyle {
                        line_height: other,
                        ..TextStyle::default()
                    },
                ),
                (
                    TextStyle {
                        letter_spacing: value,
                        ..TextStyle::default()
                    },
                    TextStyle {
                        letter_spacing: other,
                        ..TextStyle::default()
                    },
                ),
            ] {
                assert_ne!(
                    plain_measure_key("text", &first, 100.0, 7),
                    plain_measure_key("text", &second, 100.0, 7)
                );
                assert_ne!(
                    rich_measure_key(&spans, &first, 100.0, 7),
                    rich_measure_key(&spans, &second, 100.0, 7)
                );
            }
            for (first, second) in [
                (
                    StyledSpan {
                        font_size: Some(value),
                        ..span("text")
                    },
                    StyledSpan {
                        font_size: Some(other),
                        ..span("text")
                    },
                ),
                (
                    StyledSpan {
                        letter_spacing: value,
                        ..span("text")
                    },
                    StyledSpan {
                        letter_spacing: other,
                        ..span("text")
                    },
                ),
            ] {
                let style = TextStyle::default();
                assert_ne!(
                    rich_measure_key(&[first], &style, 100.0, 7),
                    rich_measure_key(&[second], &style, 100.0, 7)
                );
            }
        }
    }
}
