//! Breaking text into lines at spaces, cutting a line short with an ellipsis, measured with
//! the thread's text engine and usable outside composition, such as by layout code that has
//! to know how many lines a text takes before it is emitted.

use std::cell::RefCell;
use std::collections::HashMap;

use rustle_graphics::TextWrap;

use crate::TextStyle;
use crate::engine::{font_epoch, layout_text};

/// One line of wrapped text, as byte offsets into the text it was cut from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WrappedLine {
    /// Where the line starts.
    pub start: usize,
    /// Where it ends, after any spaces it keeps at the break but before a line break.
    pub end: usize,
    /// Where the next line starts: the same place when the line was cut for want of room,
    /// past the line break when the text had one.
    pub next: usize,
}

/// How many wrapped texts are kept before the kept ones are dropped.
const WRAPPED_MOST: usize = 4096;

/// Longer stretches of a word than this many bytes are not measured whole before cutting.
const LONGEST_MEASURED: usize = 256;

/// What a text is wrapped with besides its width: the fonts' epoch and what of the style
/// changes how wide its letters are.
type WrapKey = (u64, String, u32, u16, bool, u32, bool, u32);

thread_local! {
    /// Texts wrapped so far, by what they were wrapped with.
    static WRAPPED: RefCell<HashMap<WrapKey, HashMap<String, Vec<WrappedLine>>>> =
        RefCell::new(HashMap::new());
}

/// The key `content` wrapped at `width` in `style` is kept under.
fn key_of(style: &TextStyle, width: f32) -> WrapKey {
    (
        font_epoch(),
        format!("{:?}", style.font_family),
        style.font_size.to_bits(),
        style.font_weight.0,
        style.italic,
        style.letter_spacing.to_bits(),
        style.ligatures,
        width.to_bits(),
    )
}

/// The lines `content` breaks into at `width` logical pixels when set in `style`. The lines
/// tile the text and at least one is always returned.
///
/// A line break always breaks a line, and lines otherwise break only at spaces, each line
/// keeping the spaces after its last word; a word wider than the width is cut where it must
/// be. Results are kept per thread, so wrapping the same text again is cheap; they are
/// dropped when fonts change.
pub fn wrap_at_spaces(content: &str, style: &TextStyle, width: f32) -> Vec<WrappedLine> {
    let key = key_of(style, width);
    if let Some(lines) =
        WRAPPED.with_borrow(|held| held.get(&key).and_then(|items| items.get(content)).cloned())
    {
        return lines;
    }
    let lines = wrap_uncached(content, style, width);
    WRAPPED.with_borrow_mut(|held| {
        if held.values().map(HashMap::len).sum::<usize>() >= WRAPPED_MOST {
            held.clear();
        }
        held.entry(key)
            .or_default()
            .insert(content.to_owned(), lines.clone());
    });
    lines
}

/// The lines `content` breaks into at `width` when set in `style`, measured afresh.
fn wrap_uncached(content: &str, style: &TextStyle, width: f32) -> Vec<WrappedLine> {
    let style = TextStyle {
        max_lines: None,
        wrap: TextWrap::None,
        ..*style
    };
    let mut fits = |piece: &str| layout_text(piece.trim_end(), &style, None).size().width <= width;
    let mut lines = Vec::new();
    let mut paragraph = 0;
    for source in content.split('\n') {
        let end = paragraph + source.len();
        cut(source, paragraph, &mut fits, &mut lines);
        if let Some(last) = lines.last_mut() {
            last.next = end + 1;
        }
        paragraph = end + 1;
    }
    lines
}

/// Cuts the paragraph `source`, which starts at byte `base` of the text, into lines that
/// `fits`, breaking after the spaces that end words and cutting a word too long for a line
/// where it must be.
fn cut(
    source: &str,
    base: usize,
    fits: &mut impl FnMut(&str) -> bool,
    lines: &mut Vec<WrappedLine>,
) {
    let mut start = 0;
    let mut cursor = 0;
    for word_end in word_ends(source) {
        if word_end - start <= LONGEST_MEASURED && fits(&source[start..word_end]) {
            cursor = word_end;
            continue;
        }
        if cursor > start {
            lines.push(WrappedLine {
                start: base + start,
                end: base + cursor,
                next: base + cursor,
            });
            start = cursor;
        }
        loop {
            let split = start + fitting_prefix(&source[start..word_end], fits);
            if split >= word_end {
                break;
            }
            lines.push(WrappedLine {
                start: base + start,
                end: base + split,
                next: base + split,
            });
            start = split;
        }
        cursor = word_end;
    }
    lines.push(WrappedLine {
        start: base + start,
        end: base + source.len(),
        next: base + source.len(),
    });
}

/// The length of the longest start of `source` that `fits`, at least one character, found
/// with few measurements so a long unbroken word is not measured at every length.
fn fitting_prefix(source: &str, fits: &mut impl FnMut(&str) -> bool) -> usize {
    let mut boundaries = Vec::with_capacity(64);
    let mut good = 0;
    let mut next = 1;
    for (at, character) in source.char_indices() {
        boundaries.push(at + character.len_utf8());
        if boundaries.len() == next || at + character.len_utf8() == source.len() {
            if fits(&source[..boundaries[boundaries.len() - 1]]) {
                good = boundaries.len();
                next *= 2;
            } else {
                break;
            }
        }
    }
    if good == boundaries.len() {
        return source.len();
    }
    let mut bad = boundaries.len();
    while good + 1 < bad {
        let middle = (good + bad) / 2;
        if fits(&source[..boundaries[middle - 1]]) {
            good = middle;
        } else {
            bad = middle;
        }
    }
    boundaries[good.max(1) - 1]
}

/// Where each word of `source` ends, the spaces after it taken along.
fn word_ends(source: &str) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut spaced = false;
    for (at, character) in source.char_indices() {
        if character == ' ' {
            spaced = true;
        } else if spaced {
            ends.push(at);
            spaced = false;
        }
    }
    ends.push(source.len());
    ends
}

/// `content` set in `style`, cut short to the most of it that fits in `room` logical pixels
/// with an ellipsis after it, the spaces before the ellipsis left out; `content` itself when
/// it fits, or overflows by no more than `slack` pixels, which a box sized to its line's
/// rounded-up width allows.
pub fn ellipsize(content: &str, style: &TextStyle, room: f32, slack: f32) -> String {
    let width = |text: &str| layout_text(text, style, None).size().width;
    if width(content) <= room + slack {
        return content.to_owned();
    }
    let boundaries: Vec<usize> = content
        .char_indices()
        .map(|(at, _)| at)
        .chain(std::iter::once(content.len()))
        .collect();
    let (mut low, mut high) = (0, boundaries.len() - 1);
    while low < high {
        let middle = (low + high).div_ceil(2);
        let candidate = format!("{}…", content[..boundaries[middle]].trim_end());
        if width(&candidate) <= room {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    format!("{}…", content[..boundaries[low]].trim_end())
}

#[cfg(test)]
mod tests {
    use super::{WrappedLine, word_ends};

    /// Words end after the spaces that follow them, and the last at the end of the text.
    #[test]
    fn words_end_after_their_spaces() {
        assert_eq!(word_ends("ab  cd e"), vec![4, 7, 8]);
        assert_eq!(word_ends(""), vec![0]);
    }

    /// A wrapped line is plain data that compares by its offsets.
    #[test]
    fn lines_compare_by_offsets() {
        let line = WrappedLine {
            start: 0,
            end: 3,
            next: 4,
        };
        assert_eq!(line, WrappedLine { ..line });
    }
}
