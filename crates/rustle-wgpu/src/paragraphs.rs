//! Handing text to cosmic-text so it shapes as little as it can.
//!
//! Two things keep shaping cheap however text is styled and however often it is set again:
//!
//! - Runs are merged before cosmic-text sees them: neighbouring runs set alike become one,
//!   and the style covering most of the text becomes the default, which cosmic-text keeps no
//!   span for. Cosmic-text checks every word against every span of its paragraph, so a long
//!   paragraph with a span per run, such as one fading in as it streams, otherwise costs the
//!   product of its words and its runs.
//! - Paragraphs are kept shaped and laid out between texts, by their text, their runs and
//!   how they are laid out. Text set again, such as a reply that grows as it streams in or
//!   the same text measured, laid out and drawn in one frame, shapes only the paragraphs it
//!   did not have before, and lays out again only those set at another width. The kept
//!   paragraphs are let go when the fonts change, and the least recently used once they hold
//!   more than [`KEPT_BYTES`] of text.
//! - A long paragraph that grows, as a reply streaming in does, is handed over as pieces cut
//!   where it wrapped the last time it was laid out at the same width: every line but the
//!   last two before the text changed is a piece of its own, kept shaped and laid out, so
//!   only the lines at its end are shaped again. Greedy wrapping of left-to-right text breaks
//!   a line the same way whatever follows the next line, so the pieces lay out exactly as the
//!   whole paragraph would. A piece that a paragraph goes on from is marked with
//!   [`CONTINUES`], so its last line is not taken for the end of the paragraph.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher};
use std::ops::Range;

use glyphon::cosmic_text::{Align, BufferLine};
use glyphon::{Attrs, Buffer, Color, FontSystem, Shaping};
use rustle_graphics::FontFamily;

/// How much paragraph text, in bytes, is kept shaped before the least recently used
/// paragraphs are let go. Each byte holds a shaped and a laid-out glyph or so, a few hundred
/// bytes all told.
const KEPT_BYTES: usize = 256 * 1024;

/// A hasher for the keys paragraphs are kept by: fast over long text, and with a fixed seed,
/// so keys are the same from run to run.
fn hasher() -> foldhash::quality::FoldHasher<'static> {
    foldhash::quality::FixedState::with_seed(0x7275_7374_6c65).build_hasher()
}

/// How many leading bytes `a` and `b` have in common.
fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    const CHUNK: usize = 64;
    let length = a.len().min(b.len());
    let mut same = 0;
    while same + CHUNK <= length && a[same..same + CHUNK] == b[same..same + CHUNK] {
        same += CHUNK;
    }
    same + a[same..length]
        .iter()
        .zip(&b[same..length])
        .take_while(|(left, right)| left == right)
        .count()
}

/// The metadata a piece of a paragraph is marked with when the paragraph goes on in the
/// next line of the buffer.
const CONTINUES: usize = usize::MAX - 0x5157;

/// How long, in bytes, a paragraph must be before it is handed over in pieces.
const SPLIT_BYTES: usize = 384;

/// How many laid-out paragraphs are kept, for each way of laying them out, to find the one a
/// growing paragraph grew from.
const GROWN_KEPT: usize = 8;

/// How many ways of laying paragraphs out are kept paragraphs for before all are let go.
const GROWN_CONTEXTS: usize = 64;

/// How one run of text is set: everything cosmic-text is told about it.
#[derive(Clone, Copy, Debug)]
pub(super) struct RunStyle {
    /// The family it is set in.
    pub(super) family: FontFamily,
    /// The weight it is set at, from 100 to 900.
    pub(super) weight: u16,
    /// Whether it is slanted.
    pub(super) italic: bool,
    /// Extra space after every character, in logical pixels.
    pub(super) letter_spacing: f32,
    /// Whether the font's ligatures are used.
    pub(super) ligatures: bool,
    /// The size of the text it is in, which letter spacing is told relative to.
    pub(super) size: f32,
    /// Its own size and line height, when it differs from the text's.
    pub(super) own_size: Option<(f32, f32)>,
    /// Its colour, or `None` to draw it in the colour the text is drawn in.
    pub(super) color: Option<rustle_graphics::Color>,
}

impl RunStyle {
    /// Whether `self` and `other` are set exactly alike, comparing sizes by their bits.
    fn same(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }

    /// Everything that sets the run apart, with sizes as their bits so it can be compared
    /// and hashed exactly.
    #[allow(clippy::type_complexity)]
    fn identity(
        &self,
    ) -> (
        (u8, &'static str),
        u16,
        bool,
        u32,
        bool,
        u32,
        Option<(u32, u32)>,
        Option<(u8, u8, u8, u8)>,
    ) {
        let family = match self.family {
            FontFamily::SansSerif => (0, ""),
            FontFamily::Serif => (1, ""),
            FontFamily::Monospace => (2, ""),
            FontFamily::Named(name) => (3, name),
        };
        (
            family,
            self.weight,
            self.italic,
            self.letter_spacing.to_bits(),
            self.ligatures,
            self.size.to_bits(),
            self.own_size
                .map(|(size, height)| (size.to_bits(), height.to_bits())),
            self.color.map(|color| (color.r, color.g, color.b, color.a)),
        )
    }

    /// Feeds everything that sets the run apart into `hasher`.
    fn hash(&self, hasher: &mut impl Hasher) {
        self.identity().hash(hasher);
    }

    /// The attributes cosmic-text sets the run with.
    pub(super) fn attrs(&self) -> Attrs<'static> {
        let attrs = super::text::attributes(
            self.family,
            self.weight,
            self.italic,
            self.letter_spacing,
            self.ligatures,
            self.size,
            self.own_size,
        );
        match self.color {
            Some(color) => attrs.color(Color::rgba(color.r, color.g, color.b, color.a)),
            None => attrs,
        }
    }
}

/// A text as the runs it is set in, neighbouring runs set alike merged into one.
pub(super) struct Runs {
    /// The whole text.
    text: String,
    /// Each run's bytes of [`Runs::text`] and how it is set, in order, none empty.
    runs: Vec<(Range<usize>, RunStyle)>,
    /// A hash of each run's style, in the order of [`Runs::runs`].
    hashes: Vec<u64>,
    /// The run style covering most of the text, which cosmic-text is given as the default.
    default: RunStyle,
}

impl Runs {
    /// `pieces`, each a piece of text and how it is set, as runs: empty pieces are dropped
    /// and neighbours set alike merged. `fallback` is the default when no piece has text,
    /// or when every piece has a size of its own, which the default never has so that an
    /// empty line keeps the text's line height.
    pub(super) fn new<'a>(
        pieces: impl IntoIterator<Item = (&'a str, RunStyle)>,
        fallback: RunStyle,
    ) -> Self {
        let mut text = String::new();
        let mut runs: Vec<(Range<usize>, RunStyle)> = Vec::new();
        for (piece, style) in pieces {
            if piece.is_empty() {
                continue;
            }
            let start = text.len();
            text.push_str(piece);
            match runs.last_mut() {
                Some((range, last)) if last.same(&style) => range.end = text.len(),
                _ => runs.push((start..text.len(), style)),
            }
        }
        let default = Self::widest(&runs).unwrap_or(fallback);
        let hashes = runs
            .iter()
            .map(|(_, style)| {
                let mut hasher = hasher();
                style.hash(&mut hasher);
                hasher.finish()
            })
            .collect();
        Self {
            text,
            runs,
            hashes,
            default,
        }
    }

    /// The style of the runs that together cover the most bytes, among those without a size
    /// of their own.
    fn widest(runs: &[(Range<usize>, RunStyle)]) -> Option<RunStyle> {
        let mut covered: Vec<(RunStyle, usize)> = Vec::new();
        for (range, style) in runs.iter().filter(|(_, style)| style.own_size.is_none()) {
            if let Some((_, bytes)) = covered.iter_mut().find(|(seen, _)| seen.same(style)) {
                *bytes += range.len();
            } else if covered.len() < 16 {
                covered.push((*style, range.len()));
            }
        }
        covered
            .into_iter()
            .max_by_key(|(_, bytes)| *bytes)
            .map(|(style, _)| style)
    }

    /// Sets `buffer` to the runs, aligned as `alignment` says.
    pub(super) fn set(&self, buffer: &mut Buffer, alignment: Option<Align>) {
        buffer.set_rich_text(
            self.runs
                .iter()
                .map(|(range, style)| (&self.text[range.clone()], style.attrs())),
            &self.default.attrs(),
            Shaping::Advanced,
            alignment,
        );
    }

    /// The runs over `range` of the text, clipped to it and counted from its start, each
    /// with the hash of its style.
    fn styles_over(&self, range: Range<usize>) -> Vec<(Range<usize>, u64)> {
        let first = self.runs.partition_point(|(run, _)| run.end <= range.start);
        self.runs[first..]
            .iter()
            .zip(&self.hashes[first..])
            .take_while(|((run, _), _)| run.start < range.end)
            .map(|((run, _), hash)| {
                (
                    run.start.max(range.start) - range.start..run.end.min(range.end) - range.start,
                    *hash,
                )
            })
            .collect()
    }

    /// A key for the piece of a paragraph of `buffer` at `range` of the text, holding
    /// `line`: its text, its line ending, whether the paragraph goes on after it, the runs
    /// over it and how it is aligned.
    fn paragraph_key(&self, line: &BufferLine, range: Range<usize>, alignment: u8) -> u64 {
        let mut hasher = hasher();
        line.text().hash(&mut hasher);
        line.ending().as_str().hash(&mut hasher);
        continues(line).hash(&mut hasher);
        alignment.hash(&mut hasher);
        self.default.hash(&mut hasher);
        let first = self.runs.partition_point(|(run, _)| run.end < range.start);
        for ((run, _), hash) in self.runs[first..]
            .iter()
            .zip(&self.hashes[first..])
            .take_while(|((run, _), _)| run.start <= range.end)
        {
            (
                run.start.max(range.start) - range.start,
                run.end.min(range.end) - range.start,
                hash,
            )
                .hash(&mut hasher);
        }
        hasher.finish()
    }
}

/// Whether `line` is a piece of a paragraph that goes on in the next line of its buffer, so
/// its last line ends at a wrap rather than at the end of the paragraph.
pub(super) fn continues(line: &BufferLine) -> bool {
    line.metadata() == Some(CONTINUES)
}

/// The glyphs of `run`, a line of `buffer`, as cosmic-text would lay them out had the
/// paragraph not been cut: the last line of a piece a paragraph goes on from leaves out the
/// spaces it ends in, as a line the paragraph wrapped after does.
pub(super) fn line_glyphs<'a>(
    buffer: &Buffer,
    run: &glyphon::LayoutRun<'a>,
) -> &'a [glyphon::LayoutGlyph] {
    let glyphs = run.glyphs;
    let goes_on = buffer.lines.get(run.line_i).is_some_and(continues);
    if !goes_on
        || run.rtl
        || glyphs
            .last()
            .is_none_or(|glyph| glyph.end != run.text.len())
    {
        return glyphs;
    }
    let blank = |glyph: &glyphon::LayoutGlyph| {
        run.text
            .get(glyph.start..glyph.end)
            .is_some_and(|text| text.chars().all(char::is_whitespace))
    };
    let kept = glyphs.len() - glyphs.iter().rev().take_while(|glyph| blank(glyph)).count();
    &glyphs[..kept]
}

/// How wide `run`, a line of `buffer`, is: the width cosmic-text gives it, except that the
/// last line of a piece a paragraph goes on from is measured without the spaces it ends
/// in, as cosmic-text measures a line the paragraph wrapped after, rather than with them,
/// as it measures the last line of a paragraph.
pub(super) fn line_width(buffer: &Buffer, run: &glyphon::LayoutRun<'_>) -> f32 {
    let kept = line_glyphs(buffer, run).len();
    run.glyphs.get(kept).map_or(run.line_w, |glyph| glyph.x)
}

/// A long paragraph as it was last laid out one way, to cut the paragraph it grows into
/// where this one wrapped.
struct Grown {
    /// Its text.
    text: String,
    /// Its runs, counted from its start, each with the hash of its style.
    styles: Vec<(Range<usize>, u64)>,
    /// Where each of its lines after the first starts, in order: only lines that start
    /// after a space, at a word, are counted.
    splits: Vec<usize>,
}

impl Grown {
    /// How many leading bytes `text`, set in `styles`, has in common with this paragraph,
    /// in both its text and how each byte is set.
    fn agreeing(&self, text: &str, styles: &[(Range<usize>, u64)]) -> usize {
        let mut same = common_prefix(self.text.as_bytes(), text.as_bytes());
        for (old, new) in self.styles.iter().zip(styles) {
            if old.0.start >= same {
                break;
            }
            if old.1 != new.1 || old.0.start != new.0.start {
                same = same.min(old.0.start.min(new.0.start));
                break;
            }
            if old.0.end != new.0.end {
                same = same.min(old.0.end.min(new.0.end));
                break;
            }
        }
        same
    }

    /// Where a paragraph that has its first `same` bytes in common with this one can be cut
    /// so that each piece lays out as the lines it held here: at the start of every line
    /// whose next line also starts within those bytes, since a greedy wrap breaks a line
    /// the same way as long as the line and the first word after it are the same.
    fn usable(&self, same: usize) -> &[usize] {
        let count = self
            .splits
            .windows(2)
            .take_while(|pair| pair[1] <= same)
            .count();
        &self.splits[..count]
    }
}

/// Whether a paragraph saying `text` can be handed over in pieces cut where it wraps: it is
/// long, and holds no tab, no right-to-left script and nothing else whose shape or direction
/// depends on what comes before it in its line.
fn splittable(text: &str) -> bool {
    text.len() >= SPLIT_BYTES
        && text.chars().all(|character| {
            let code = u32::from(character);
            character != '\t'
                && !(0x0590..=0x08FF).contains(&code)
                && !(0x200E..=0x200F).contains(&code)
                && !(0x202A..=0x202E).contains(&code)
                && !(0x2066..=0x2069).contains(&code)
                && !(0xFB1D..=0xFEFF).contains(&code)
                && !(0x10800..=0x10FFF).contains(&code)
                && !(0x1E800..=0x1EFFF).contains(&code)
        })
}

thread_local! {
    /// Long paragraphs as last laid out, by a key for how they were laid out and set.
    static GROWN: RefCell<HashMap<u64, Vec<Grown>>> = RefCell::new(HashMap::new());
}

/// Where to cut a paragraph saying `text` set in `styles`, laid out as `context` says: where
/// the paragraph it most likely grew from wrapped, as far as the two agree.
fn cuts_for(context: u64, text: &str, styles: &[(Range<usize>, u64)]) -> Vec<usize> {
    GROWN.with(|grown| {
        let grown = grown.borrow();
        let Some(candidates) = grown.get(&context) else {
            return Vec::new();
        };
        candidates
            .iter()
            .map(|candidate| candidate.usable(candidate.agreeing(text, styles)))
            .max_by_key(|cuts| cuts.len())
            .map(<[usize]>::to_vec)
            .unwrap_or_default()
    })
}

/// Keeps how a long paragraph saying `text` set in `styles`, laid out as `context` says,
/// wrapped: `splits` are where its lines after the first start.
fn remember_grown(context: u64, text: &str, styles: Vec<(Range<usize>, u64)>, splits: Vec<usize>) {
    GROWN.with(|grown| {
        let mut grown = grown.borrow_mut();
        if grown.len() >= GROWN_CONTEXTS && !grown.contains_key(&context) {
            grown.clear();
        }
        let candidates = grown.entry(context).or_default();
        if let Some(found) = candidates
            .iter()
            .position(|candidate| candidate.text == text && candidate.styles == styles)
        {
            let mut same = candidates.remove(found);
            same.splits = splits;
            candidates.insert(0, same);
            return;
        }
        candidates.insert(
            0,
            Grown {
                text: text.to_owned(),
                styles,
                splits,
            },
        );
        candidates.truncate(GROWN_KEPT);
    });
}

/// `line`, a paragraph, cut at `cuts`, ascending byte offsets into it: the pieces in order,
/// each but the last marked as going on in the next.
fn cut(mut line: BufferLine, cuts: &[usize]) -> Vec<BufferLine> {
    let mut tails: Vec<BufferLine> = cuts.iter().rev().map(|at| line.split_off(*at)).collect();
    tails.push(line);
    tails.reverse();
    let last = tails.len() - 1;
    for piece in &mut tails[..last] {
        piece.set_metadata(CONTINUES);
    }
    tails
}

/// Where the lines of a paragraph, laid out as `pieces`, start after its first, when they
/// start after a space of `text`, the paragraph's text.
fn line_starts(pieces: &[BufferLine], text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut starts = Vec::new();
    let mut offset = 0;
    for piece in pieces {
        if offset > 0 {
            starts.push(offset);
        }
        for line in piece.layout_opt().into_iter().flatten().skip(1) {
            let Some(start) = line.glyphs.iter().map(|glyph| glyph.start).min() else {
                continue;
            };
            let at = offset + start;
            if at > 0 && bytes.get(at - 1) == Some(&b' ') {
                starts.push(at);
            }
        }
        offset += piece.text().len();
    }
    starts.sort_unstable();
    starts.dedup();
    starts
}

/// A paragraph kept shaped and laid out.
struct KeptLine {
    /// The paragraph, shaped and laid out.
    line: BufferLine,
    /// When it was last used, by the clock of [`Kept`].
    used: u64,
    /// How many bytes of text it counts for against [`KEPT_BYTES`].
    bytes: usize,
}

/// The paragraphs kept shaped on this thread.
#[derive(Default)]
struct Kept {
    /// The paragraphs, by [`Runs::paragraph_key`] and [`layout_key`].
    lines: HashMap<(u64, u64), KeptLine>,
    /// The layout each paragraph was last kept with, by [`Runs::paragraph_key`], to find it
    /// shaped when it is laid out another way.
    latest: HashMap<u64, u64>,
    /// Counts up with every text set, to tell which paragraphs were used last.
    clock: u64,
    /// The bytes of text the paragraphs count for.
    bytes: usize,
    /// How many faces the font system held, and the font epoch, when they were shaped.
    fonts: (usize, u64),
}

impl Kept {
    /// The paragraph kept under `key`, laid out as `layout` says, marked used now: moved
    /// out when `borrow` is set, copied otherwise.
    fn take(&mut self, key: u64, layout: u64, borrow: bool) -> Option<BufferLine> {
        let clock = self.clock;
        if borrow {
            let found = self.lines.remove(&(key, layout))?;
            self.bytes -= found.bytes;
            return Some(found.line);
        }
        let found = self.lines.get_mut(&(key, layout))?;
        found.used = clock;
        Some(found.line.clone())
    }

    /// Keeps `line` under `key`, laid out as `layout` says, as the latest layout of it.
    fn keep(&mut self, key: u64, layout: u64, line: BufferLine) {
        let bytes = line.text().len().max(16);
        self.latest.insert(key, layout);
        let old = self.lines.insert(
            (key, layout),
            KeptLine {
                line,
                used: self.clock,
                bytes,
            },
        );
        self.bytes += bytes;
        if let Some(old) = old {
            self.bytes -= old.bytes;
        }
    }

    /// Lets go of the least recently used paragraphs when they hold more than
    /// [`KEPT_BYTES`].
    fn trim_if_full(&mut self) {
        if self.bytes > KEPT_BYTES {
            self.trim();
        }
    }

    /// Lets go of the least recently used paragraphs until they hold at most three
    /// quarters of [`KEPT_BYTES`].
    fn trim(&mut self) {
        let mut ages: Vec<(u64, (u64, u64), usize)> = self
            .lines
            .iter()
            .map(|(key, kept)| (kept.used, *key, kept.bytes))
            .collect();
        ages.sort_unstable();
        for (_, key, bytes) in ages {
            if self.bytes <= KEPT_BYTES * 3 / 4 {
                break;
            }
            self.lines.remove(&key);
            if self.latest.get(&key.0) == Some(&key.1) {
                self.latest.remove(&key.0);
            }
            self.bytes -= bytes;
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Whether text is shaped whole, with nothing kept or cut, for tests to compare against.
    static WHOLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// How many paragraphs were cut into pieces on this thread.
    static CUT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many paragraphs were cut into pieces on this thread, for tests.
#[cfg(test)]
pub(super) fn paragraphs_cut() -> usize {
    CUT.with(std::cell::Cell::get)
}

/// Runs `run` with text shaped whole on this thread, nothing kept from before or kept for
/// later and no paragraph cut, as text would be shaped without this module.
#[cfg(test)]
pub(super) fn shaped_whole<T>(run: impl FnOnce() -> T) -> T {
    WHOLE.with(|whole| whole.set(true));
    let result = run();
    WHOLE.with(|whole| whole.set(false));
    result
}

/// Whether text is shaped whole on this thread; see [`shaped_whole`].
fn whole() -> bool {
    #[cfg(test)]
    return WHOLE.with(std::cell::Cell::get);
    #[cfg(not(test))]
    false
}

thread_local! {
    /// The paragraphs kept shaped on this thread.
    static KEPT: RefCell<Kept> = RefCell::new(Kept::default());
}

/// A key for how lines are laid out: the text's size and line height, the width it wraps
/// at, how it wraps and how many lines it is cut to.
pub(super) fn layout_key(
    metrics: (f32, f32),
    width: Option<f32>,
    wrap: rustle_graphics::TextWrap,
    max_lines: Option<usize>,
) -> u64 {
    let mut hasher = hasher();
    metrics.0.to_bits().hash(&mut hasher);
    metrics.1.to_bits().hash(&mut hasher);
    width.map(f32::to_bits).hash(&mut hasher);
    std::mem::discriminant(&wrap).hash(&mut hasher);
    max_lines.hash(&mut hasher);
    hasher.finish()
}

/// A number for `alignment`, for keys.
fn alignment_number(alignment: Option<Align>) -> u8 {
    match alignment {
        None => 0,
        Some(Align::Left) => 1,
        Some(Align::Center) => 2,
        Some(Align::Right) => 3,
        Some(Align::Justified) => 4,
        Some(Align::End) => 5,
    }
}

/// A paragraph of a buffer handed over in pieces, or whole as one piece.
struct Paragraph {
    /// The index of its first piece among the buffer's lines.
    first: usize,
    /// How many pieces it was handed over as.
    pieces: usize,
    /// Its bytes of the whole text.
    range: Range<usize>,
}

/// Cuts the long paragraphs of `buffer`, which holds `runs` aligned as `alignment` numbers,
/// where the paragraphs they grew from wrapped when laid out as `context` says, and returns
/// the long paragraphs. A paragraph kept shaped whole is not cut: laying it out again costs
/// less than shaping pieces of it.
fn cut_paragraphs(buffer: &mut Buffer, runs: &Runs, alignment: u8, context: u64) -> Vec<Paragraph> {
    let mut long = Vec::new();
    let whole = std::mem::take(&mut buffer.lines);
    let mut start = 0;
    for line in whole {
        let range = start..start + line.text().len();
        start = range.end + line.ending().as_str().len();
        let first = buffer.lines.len();
        if !splittable(line.text())
            || KEPT.with(|kept| {
                let key = runs.paragraph_key(&line, range.clone(), alignment);
                kept.borrow().latest.contains_key(&key)
            })
        {
            buffer.lines.push(line);
            continue;
        }
        let cuts = cuts_for(
            context,
            &runs.text[range.clone()],
            &runs.styles_over(range.clone()),
        );
        if cuts.is_empty() {
            buffer.lines.push(line);
        } else {
            #[cfg(test)]
            CUT.with(|count| count.set(count.get() + 1));
            buffer.lines.extend(cut(line, &cuts));
        }
        long.push(Paragraph {
            first,
            pieces: buffer.lines.len() - first,
            range,
        });
    }
    long
}

/// The paragraphs a buffer took from those kept rather than copied, to be given back with
/// [`give_back`] once it is read.
pub(super) struct Borrowed {
    /// The key of each line of the buffer; see [`Runs::paragraph_key`].
    keys: Vec<u64>,
    /// How the buffer was laid out; see [`layout_key`].
    layout: u64,
}

/// Shapes and lays out `buffer`, which holds `runs` aligned as `alignment` says, laid out
/// as `layout` identifies: long paragraphs are cut where they wrapped before when `growing`
/// (the text wraps and is not cut to a number of lines), and paragraphs kept from before are
/// put in their place, laid out again only when they were laid out otherwise.
///
/// When `borrow` is set the kept paragraphs are moved into `buffer` rather than copied, and
/// what it returns must be given back with the buffer to [`give_back`] once the buffer is
/// read, which keeps them all, those shaped here included; a buffer that is read once and
/// dropped, as one measured is, so costs no copy of its paragraphs. Otherwise the paragraphs
/// are copied into `buffer` and those shaped or laid out here are kept at once, for a buffer
/// that is kept for drawing.
pub(super) fn shape_kept(
    font_system: &mut FontSystem,
    buffer: &mut Buffer,
    runs: &Runs,
    (alignment, layout): (Option<Align>, u64),
    growing: bool,
    borrow: bool,
) -> Option<Borrowed> {
    if whole() {
        buffer.shape_until_scroll(font_system, false);
        return None;
    }
    let fonts = (font_system.db().len(), rustle_text::font_epoch());
    let alignment = alignment_number(alignment);
    let context = {
        let mut hasher = hasher();
        (layout, alignment, fonts).hash(&mut hasher);
        runs.default.hash(&mut hasher);
        hasher.finish()
    };
    KEPT.with(|kept| {
        let mut kept = kept.borrow_mut();
        if kept.fonts != fonts {
            *kept = Kept {
                fonts,
                ..Kept::default()
            };
        }
    });
    let long = if growing {
        cut_paragraphs(buffer, runs, alignment, context)
    } else {
        Vec::new()
    };
    let mut keys = Vec::with_capacity(buffer.lines.len());
    KEPT.with(|kept| {
        let mut kept = kept.borrow_mut();
        kept.clock += 1;
        let mut start = 0;
        for line in &mut buffer.lines {
            let range = start..start + line.text().len();
            start = range.end + line.ending().as_str().len();
            let key = runs.paragraph_key(line, range, alignment);
            let pieced = continues(line);
            let fresh = if let Some(found) = kept.take(key, layout, borrow) {
                *line = found;
                false
            } else if let Some(other) = kept.latest.get(&key).copied()
                && let Some(found) = kept.take(key, other, borrow)
            {
                *line = found;
                line.reset_layout();
                true
            } else {
                true
            };
            if pieced {
                line.set_metadata(CONTINUES);
            }
            keys.push((key, fresh));
        }
    });
    buffer.shape_until_scroll(font_system, false);
    if !borrow {
        KEPT.with(|kept| {
            let mut kept = kept.borrow_mut();
            for (line, (key, fresh)) in buffer.lines.iter().zip(&keys) {
                if *fresh && line.layout_opt().is_some() {
                    kept.keep(*key, layout, line.clone());
                }
            }
            kept.trim_if_full();
        });
    }
    for paragraph in long {
        let text = &runs.text[paragraph.range.clone()];
        let pieces = &buffer.lines[paragraph.first..paragraph.first + paragraph.pieces];
        let starts = line_starts(pieces, text);
        remember_grown(context, text, runs.styles_over(paragraph.range), starts);
    }
    borrow.then(|| Borrowed {
        keys: keys.into_iter().map(|(key, _)| key).collect(),
        layout,
    })
}

/// Keeps the paragraphs of `buffer`, which [`shape_kept`] shaped and lent `borrowed` for,
/// for next time.
pub(super) fn give_back(buffer: Buffer, borrowed: Borrowed) {
    KEPT.with(|kept| {
        let mut kept = kept.borrow_mut();
        for (line, key) in buffer.lines.into_iter().zip(borrowed.keys) {
            if line.layout_opt().is_some() {
                kept.keep(key, borrowed.layout, line);
            }
        }
        kept.trim_if_full();
    });
}
