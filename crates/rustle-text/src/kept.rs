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

/// How many bytes of text one generation holds before it is retired.
const GENERATION_BYTES: usize = 8 << 20;

/// Text longer than this many bytes is shaped every time, not kept: such text is usually
/// being edited, so it is rarely asked for twice.
const LONGEST_KEPT: usize = 32 << 10;

/// What a layout was made from, compared in full on a lookup so a hash collision can never
/// hand back the layout of other text.
#[derive(PartialEq)]
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
    /// Takes the layout made from `made`, kept under `hash`, out of this generation.
    fn take(&mut self, hash: u64, made: &Made) -> Option<(Made, TextLayout)> {
        let bucket = self.layouts.get_mut(&hash)?;
        let index = bucket.iter().position(|(kept, _)| kept == made)?;
        let entry = bucket.swap_remove(index);
        if bucket.is_empty() {
            self.layouts.remove(&hash);
        }
        self.count -= 1;
        self.bytes -= entry.1.text().len();
        Some(entry)
    }

    /// The layout made from `made`, kept under `hash`, in this generation.
    fn get(&self, hash: u64, made: &Made) -> Option<&TextLayout> {
        self.layouts
            .get(&hash)?
            .iter()
            .find(|(kept, _)| kept == made)
            .map(|(_, layout)| layout)
    }

    /// Keeps `layout`, made from `made`, under `hash`.
    fn keep(&mut self, hash: u64, made: Made, layout: TextLayout) {
        self.count += 1;
        self.bytes += layout.text().len();
        self.layouts.entry(hash).or_default().push((made, layout));
    }

    /// Whether this generation has no room for more.
    fn is_full(&self) -> bool {
        self.count >= GENERATION_LAYOUTS || self.bytes >= GENERATION_BYTES
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

thread_local! {
    /// The layouts kept on this thread.
    static KEPT: RefCell<Kept> = RefCell::new(Kept::default());
}

/// The layout of `spans` set in `style` and broken at `width` in the fonts of `epoch`: one
/// kept on this thread, or the one `shape` makes, which is then kept.
pub(crate) fn kept_layout(
    spans: Vec<StyledSpan>,
    style: &TextStyle,
    width: Option<f32>,
    epoch: u64,
    shape: impl FnOnce(&[StyledSpan]) -> TextLayout,
) -> TextLayout {
    let length: usize = spans.iter().map(|span| span.text.len()).sum();
    if length > LONGEST_KEPT {
        return shape(&spans);
    }
    let made = Made {
        spans,
        style: *style,
        width: width.map(f32::to_bits),
        epoch,
    };
    let hash = hash_of(&made);
    let found = KEPT.with_borrow_mut(|kept| {
        if let Some(layout) = kept.newer.get(hash, &made) {
            return Some(layout.clone());
        }
        let (made, layout) = kept.older.take(hash, &made)?;
        kept.newer.keep(hash, made, layout.clone());
        Some(layout)
    });
    if let Some(layout) = found {
        return layout;
    }
    let layout = shape(&made.spans);
    KEPT.with_borrow_mut(|kept| {
        if kept.newer.is_full() {
            kept.older = std::mem::take(&mut kept.newer);
        }
        kept.newer.keep(hash, made, layout.clone());
    });
    layout
}

/// A hash of everything `made` holds.
fn hash_of(made: &Made) -> u64 {
    let mut hasher = DefaultHasher::new();
    made.spans.len().hash(&mut hasher);
    for span in &made.spans {
        hash_span(span, &mut hasher);
    }
    hash_style(&made.style, &mut hasher);
    made.width.hash(&mut hasher);
    made.epoch.hash(&mut hasher);
    hasher.finish()
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
        kept_layout(span(text), &style, width, epoch, |spans| {
            calls.set(calls.get() + 1);
            TextLayout::new(spans[0].text.clone(), spans.to_vec(), 14.0, Vec::new())
        })
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

    /// Text past the longest kept is shaped every time.
    #[test]
    fn shapes_long_text_every_time() {
        forget();
        let calls = Cell::new(0);
        let long = "x".repeat(LONGEST_KEPT + 1);
        lay(&calls, &long, TextStyle::default(), None, 0);
        lay(&calls, &long, TextStyle::default(), None, 0);
        assert_eq!(calls.get(), 2);
    }
}
