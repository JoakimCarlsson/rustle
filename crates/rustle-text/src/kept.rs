//! Layouts of text kept per thread, so text shaped once is not shaped again while its runs,
//! style, width and fonts stay the same.
//!
//! Composables measure their text while composing, and `canvas` closures and ellipsized lines
//! measure it again while painting, every frame, for text that has not changed. Shaping is the
//! dearest part of such a frame, so layouts are kept in two generations: a lookup that hits
//! the older one moves the layout to the newer, and when the newer is full the older is
//! dropped and the newer takes its place. Text in use every frame stays, and text no longer
//! asked for leaves within two generations.

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use rustle_graphics::{Color, FontFamily, StyledSpan};

use crate::{TextLayout, TextStyle};

/// How many layouts one generation holds before it is retired.
const GENERATION_LAYOUTS: usize = 4096;

/// How many bytes of text one generation holds before it is retired. Text longer than this
/// cannot fit in a generation, so it is shaped every time.
const GENERATION_BYTES: usize = 8 << 20;

/// One run of text with its style resolved, borrowing its text: what a lookup is made from,
/// so text already kept is found without copying it.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct SpanKey<'a> {
    pub(crate) text: &'a str,
    pub(crate) color: Color,
    pub(crate) font_weight: u16,
    pub(crate) font_family: FontFamily,
    pub(crate) italic: bool,
    pub(crate) underline: bool,
    pub(crate) strikethrough: bool,
    pub(crate) background: Option<Color>,
    pub(crate) font_size: Option<f32>,
    pub(crate) letter_spacing: f32,
    pub(crate) ligatures: bool,
}

impl<'a> SpanKey<'a> {
    /// The key of `span`, borrowing its text.
    pub(crate) fn of(span: &'a StyledSpan) -> Self {
        Self {
            text: &span.text,
            color: span.color,
            font_weight: span.font_weight,
            font_family: span.font_family,
            italic: span.italic,
            underline: span.underline,
            strikethrough: span.strikethrough,
            background: span.background,
            font_size: span.font_size,
            letter_spacing: span.letter_spacing,
            ligatures: span.ligatures,
        }
    }

    /// The run `span` with what it leaves unset taken from `style`, as
    /// [`TextSpan::resolve`](crate::TextSpan::resolve) resolves it, borrowing its text.
    pub(crate) fn resolve(span: &'a crate::TextSpan, style: &TextStyle) -> Self {
        Self {
            text: &span.text,
            color: span.color.unwrap_or(style.color),
            font_weight: span.font_weight.unwrap_or(style.font_weight).0,
            font_family: span.font_family.unwrap_or(style.font_family),
            italic: span.italic.unwrap_or(style.italic),
            underline: span.underline.unwrap_or(style.underline),
            strikethrough: span.strikethrough.unwrap_or(style.strikethrough),
            background: span.background,
            font_size: span.font_size,
            letter_spacing: span.letter_spacing.unwrap_or(style.letter_spacing),
            ligatures: span.ligatures.unwrap_or(style.ligatures),
        }
    }

    /// `text` as one run in `style`, as a [`TextSpan`](crate::TextSpan) of only `text`
    /// resolves.
    pub(crate) fn plain(text: &'a str, style: &TextStyle) -> Self {
        Self {
            text,
            color: style.color,
            font_weight: style.font_weight.0,
            font_family: style.font_family,
            italic: style.italic,
            underline: style.underline,
            strikethrough: style.strikethrough,
            background: None,
            font_size: None,
            letter_spacing: style.letter_spacing,
            ligatures: style.ligatures,
        }
    }

    /// The owned run this key describes.
    fn to_span(self) -> StyledSpan {
        StyledSpan {
            text: self.text.to_owned(),
            color: self.color,
            font_weight: self.font_weight,
            font_family: self.font_family,
            italic: self.italic,
            underline: self.underline,
            strikethrough: self.strikethrough,
            background: self.background,
            font_size: self.font_size,
            letter_spacing: self.letter_spacing,
            ligatures: self.ligatures,
        }
    }
}

/// What a layout was made from, compared in full on a lookup so a hash collision can never
/// hand back the layout of other text.
struct Made {
    /// The runs, their styles resolved.
    spans: Vec<StyledSpan>,
    /// The style around them.
    style: TextStyle,
    /// The width lines were broken at, as bits; `None` for no wrapping.
    width: Option<u32>,
    /// The font epoch the text was shaped in.
    epoch: u64,
}

/// What a lookup is made from: [`Made`] with its runs borrowed.
struct Lookup<'s, I> {
    /// The runs, their styles resolved.
    spans: I,
    /// The style around them.
    style: &'s TextStyle,
    /// The width lines are broken at, as bits; `None` for no wrapping.
    width: Option<u32>,
    /// The font epoch the text is shaped in.
    epoch: u64,
}

impl<'a, I: Iterator<Item = SpanKey<'a>> + Clone> Lookup<'_, I> {
    /// Whether `made` was made from what this lookup is made from.
    fn matches(&self, made: &Made) -> bool {
        made.style == *self.style
            && made.width == self.width
            && made.epoch == self.epoch
            && same_runs(&made.spans, self.spans.clone())
    }

    /// A hash of everything this lookup is made from.
    fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.spans.clone().count().hash(&mut hasher);
        for span in self.spans.clone() {
            hash_span(&span, &mut hasher);
        }
        hash_style(self.style, &mut hasher);
        self.width.hash(&mut hasher);
        self.epoch.hash(&mut hasher);
        hasher.finish()
    }

    /// What this lookup is made from, owned, to keep with the layout made from it.
    fn to_made(&self) -> Made {
        Made {
            spans: self.spans.clone().map(SpanKey::to_span).collect(),
            style: *self.style,
            width: self.width,
            epoch: self.epoch,
        }
    }
}

/// Whether `kept` holds the runs `keys` describe, one for one.
fn same_runs<'a>(kept: &[StyledSpan], mut keys: impl Iterator<Item = SpanKey<'a>>) -> bool {
    kept.iter()
        .all(|span| keys.next().is_some_and(|key| SpanKey::of(span) == key))
        && keys.next().is_none()
}

/// One generation of kept layouts.
#[derive(Default)]
struct Generation {
    /// The layouts by the hash of what they were made from.
    layouts: HashMap<u64, Vec<(Made, TextLayout)>>,
    /// How many layouts it holds.
    count: usize,
    /// How many bytes of text they hold.
    bytes: usize,
}

impl Generation {
    /// Takes the layout made from what `lookup` is made from, kept under `hash`, out of this
    /// generation.
    fn take<'a, I: Iterator<Item = SpanKey<'a>> + Clone>(
        &mut self,
        hash: u64,
        lookup: &Lookup<'_, I>,
    ) -> Option<(Made, TextLayout)> {
        let bucket = self.layouts.get_mut(&hash)?;
        let index = bucket.iter().position(|(kept, _)| lookup.matches(kept))?;
        let entry = bucket.swap_remove(index);
        if bucket.is_empty() {
            self.layouts.remove(&hash);
        }
        self.count -= 1;
        self.bytes -= entry.1.text().len();
        Some(entry)
    }

    /// The layout made from what `lookup` is made from, kept under `hash`, in this
    /// generation.
    fn get<'a, I: Iterator<Item = SpanKey<'a>> + Clone>(
        &self,
        hash: u64,
        lookup: &Lookup<'_, I>,
    ) -> Option<&TextLayout> {
        self.layouts
            .get(&hash)?
            .iter()
            .find(|(kept, _)| lookup.matches(kept))
            .map(|(_, layout)| layout)
    }

    /// Keeps `layout`, made from `made`, under `hash`.
    fn keep(&mut self, hash: u64, made: Made, layout: TextLayout) {
        self.count += 1;
        self.bytes += layout.text().len();
        self.layouts.entry(hash).or_default().push((made, layout));
    }

    /// Whether this generation has no room for a layout of `bytes` more bytes of text.
    fn is_full_for(&self, bytes: usize) -> bool {
        self.count >= GENERATION_LAYOUTS || self.bytes + bytes > GENERATION_BYTES
    }
}

/// The two generations of kept layouts.
#[derive(Default)]
struct Kept {
    /// Where layouts are kept and looked for first.
    newer: Generation,
    /// The generation before, from which layouts still asked for move to the newer.
    older: Generation,
}

impl Kept {
    /// Keeps `layout`, made from `made`, under `hash` in the newer generation, retiring it
    /// first when it has no room. A layout of more text than a generation holds is not kept.
    fn keep(&mut self, hash: u64, made: Made, layout: TextLayout) {
        let bytes = layout.text().len();
        if bytes > GENERATION_BYTES {
            return;
        }
        if self.newer.is_full_for(bytes) {
            self.older = std::mem::take(&mut self.newer);
        }
        self.newer.keep(hash, made, layout);
    }
}

thread_local! {
    /// The layouts kept on this thread.
    static KEPT: RefCell<Kept> = RefCell::new(Kept::default());
}

/// The layout of the runs `spans` set in `style` and broken at `width` in the fonts of
/// `epoch`: one kept on this thread, or the one `shape` makes, which is then kept. The runs
/// are only copied when no layout of them is kept.
pub(crate) fn kept_layout<'a>(
    spans: impl Iterator<Item = SpanKey<'a>> + Clone,
    style: &TextStyle,
    width: Option<f32>,
    epoch: u64,
    shape: impl FnOnce(&[StyledSpan]) -> TextLayout,
) -> TextLayout {
    let lookup = Lookup {
        spans,
        style,
        width: width.map(f32::to_bits),
        epoch,
    };
    let hash = lookup.hash();
    let found = KEPT.with_borrow_mut(|kept| {
        if let Some(layout) = kept.newer.get(hash, &lookup) {
            return Some(layout.clone());
        }
        let (made, layout) = kept.older.take(hash, &lookup)?;
        kept.keep(hash, made, layout.clone());
        Some(layout)
    });
    if let Some(layout) = found {
        return layout;
    }
    let made = lookup.to_made();
    let layout = shape(&made.spans);
    KEPT.with_borrow_mut(|kept| kept.keep(hash, made, layout.clone()));
    layout
}

/// Feeds every field of `style` into `hasher`, floats by their bits.
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
    hash_color(*color, hasher);
    font_size.to_bits().hash(hasher);
    line_height.to_bits().hash(hasher);
    font_weight.0.hash(hasher);
    hash_family(*font_family, hasher);
    std::mem::discriminant(align).hash(hasher);
    max_lines.hash(hasher);
    italic.hash(hasher);
    underline.hash(hasher);
    strikethrough.hash(hasher);
    letter_spacing.to_bits().hash(hasher);
    ligatures.hash(hasher);
    wrap.hash(hasher);
}

/// Feeds the text and every style field of `span` into `hasher`, floats by their bits.
fn hash_span(span: &SpanKey<'_>, hasher: &mut impl Hasher) {
    let SpanKey {
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
    background
        .map(|color| (color.r, color.g, color.b, color.a))
        .hash(hasher);
    font_size.map(f32::to_bits).hash(hasher);
    letter_spacing.to_bits().hash(hasher);
    ligatures.hash(hasher);
}

/// Feeds the four channels of `color` into `hasher`.
fn hash_color(color: Color, hasher: &mut impl Hasher) {
    (color.r, color.g, color.b, color.a).hash(hasher);
}

/// Feeds the kind of `family`, and its name when it has one, into `hasher`.
fn hash_family(family: FontFamily, hasher: &mut impl Hasher) {
    std::mem::discriminant(&family).hash(hasher);
    if let FontFamily::Named(name) = family {
        name.hash(hasher);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Drops every layout kept on this thread, so a test starts from none.
    fn forget() {
        KEPT.with_borrow_mut(|kept| *kept = Kept::default());
    }

    /// A run of `text` in black, normal weight, sans serif.
    fn span(text: &str) -> Vec<StyledSpan> {
        vec![StyledSpan::new(
            text,
            Color::rgb(0, 0, 0),
            400,
            FontFamily::SansSerif,
        )]
    }

    /// Lays out `text` through the kept layouts, counting in `calls` how often it is shaped.
    fn lay(
        calls: &Cell<usize>,
        text: &str,
        style: TextStyle,
        width: Option<f32>,
        epoch: u64,
    ) -> TextLayout {
        let spans = span(text);
        kept_layout(
            spans.iter().map(SpanKey::of),
            &style,
            width,
            epoch,
            |spans| {
                calls.set(calls.get() + 1);
                TextLayout::new(spans[0].text.clone(), spans.to_vec(), 14.0, Vec::new())
            },
        )
    }

    /// The same text, style, width and fonts are shaped once; a change in any is shaped again.
    #[test]
    fn keeps_a_layout_until_what_it_was_made_from_changes() {
        forget();
        let calls = Cell::new(0);
        let style = TextStyle::default();
        assert_eq!(lay(&calls, "hello", style, None, 0).text(), "hello");
        assert_eq!(lay(&calls, "hello", style, None, 0).text(), "hello");
        assert_eq!(calls.get(), 1);
        lay(&calls, "hello", style, Some(40.0), 0);
        let larger = TextStyle {
            font_size: 20.0,
            ..style
        };
        lay(&calls, "hello", larger, None, 0);
        lay(&calls, "hello", style, None, 1);
        assert_eq!(lay(&calls, "help", style, None, 0).text(), "help");
        assert_eq!(calls.get(), 5);
        lay(&calls, "hello", style, Some(40.0), 0);
        assert_eq!(calls.get(), 5);
    }

    /// A full generation is retired: what is still asked for moves on, the rest is dropped.
    #[test]
    fn retires_a_full_generation() {
        forget();
        let calls = Cell::new(0);
        let style = TextStyle::default();
        lay(&calls, "kept", style, None, 0);
        for index in 0..GENERATION_LAYOUTS {
            lay(&calls, &format!("filler {index}"), style, None, 0);
        }
        lay(&calls, "kept", style, None, 0);
        assert_eq!(calls.get(), GENERATION_LAYOUTS + 1);
        for index in 0..GENERATION_LAYOUTS {
            lay(&calls, &format!("other {index}"), style, None, 0);
        }
        lay(&calls, "kept", style, None, 0);
        assert_eq!(calls.get(), 2 * GENERATION_LAYOUTS + 1);
        lay(&calls, "filler 0", style, None, 0);
        assert_eq!(calls.get(), 2 * GENERATION_LAYOUTS + 2);
    }

    /// Long text is kept like short text; text past what a generation holds is shaped every
    /// time.
    #[test]
    fn keeps_long_text_within_the_byte_budget() {
        forget();
        let calls = Cell::new(0);
        let long = "x".repeat(64 << 10);
        lay(&calls, &long, TextStyle::default(), None, 0);
        lay(&calls, &long, TextStyle::default(), None, 0);
        assert_eq!(calls.get(), 1);
        let huge = "x".repeat(GENERATION_BYTES + 1);
        lay(&calls, &huge, TextStyle::default(), None, 0);
        lay(&calls, &huge, TextStyle::default(), None, 0);
        assert_eq!(calls.get(), 3);
    }

    /// A generation is retired once the text it holds passes its byte budget.
    #[test]
    fn retires_a_generation_past_its_byte_budget() {
        forget();
        let calls = Cell::new(0);
        let style = TextStyle::default();
        let quarter = GENERATION_BYTES / 4;
        for index in 0..4 {
            lay(&calls, &index.to_string().repeat(quarter), style, None, 0);
        }
        KEPT.with_borrow(|kept| assert_eq!(kept.newer.count, 4));
        lay(&calls, "more", style, None, 0);
        KEPT.with_borrow(|kept| {
            assert_eq!(kept.newer.count, 1);
            assert_eq!(kept.older.count, 4);
        });
    }

    /// A layout found by plain text is the one found by the same text as a resolved run.
    #[test]
    fn plain_text_finds_the_resolved_run() {
        forget();
        let calls = Cell::new(0);
        let style = TextStyle::default();
        let text = crate::TextSpan::new("same");
        let shape = |spans: &[StyledSpan]| {
            calls.set(calls.get() + 1);
            TextLayout::new(spans[0].text.clone(), spans.to_vec(), 14.0, Vec::new())
        };
        kept_layout(
            std::iter::once(SpanKey::plain("same", &style)),
            &style,
            None,
            0,
            shape,
        );
        kept_layout(
            std::iter::once(SpanKey::resolve(&text, &style)),
            &style,
            None,
            0,
            shape,
        );
        let resolved = [text.resolve(&style)];
        kept_layout(resolved.iter().map(SpanKey::of), &style, None, 0, shape);
        assert_eq!(calls.get(), 1);
    }
}
