//! Lines of text with stretches painted apart from the rest: colored runs, fills behind
//! matches, wavy and straight underlines, carets, indent guides and the point where dragged
//! text would land, drawn over and under the shaped line. Made for code views, terminals and
//! diffs, which build one such line per visible row.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use rustle_graphics::Color;
use rustle_text::{Affinity, TextLayout, TextSpan, TextStyle, layout_spans, layout_text};
use rustle_ui::{Canvas, Modifier};

use crate::text::{LOCAL_TEXT_STYLE, rich_text_with};

/// The width of a bar caret, in logical pixels.
pub const CARET_WIDTH: f32 = 1.5;

/// How long each piece of a wavy underline is, in logical pixels.
const WAVE_STEP: f32 = 2.0;

/// How opaque a block caret is drawn, so the character under it shows through.
const BLOCK_CARET_ALPHA: f32 = 0.45;

/// How a caret is drawn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CaretShape {
    /// A thin upright bar before the character it is at.
    #[default]
    Bar,
    /// The cell of the character it is at, filled translucently.
    Block,
    /// A line under the character it is at.
    Underline,
}

/// How a stretch of a [`marked_text`] line is painted apart from the rest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkPaint {
    /// Its letters in the mark's color.
    Ink,
    /// A box of the color behind it, line by line, as a selection is drawn.
    Fill,
    /// A wavy line under it.
    Wave,
    /// A straight line under it.
    Underline,
    /// A caret before its first byte, shown while the line's carets are visible.
    Caret,
    /// A thin upright line the height of the line at the column its first number counts, in
    /// widths of a space from the left, which may lie past the end of the text.
    Guide,
    /// A steady upright line before its first byte: where dragged text would land.
    Drop,
}

/// A stretch of a [`marked_text`] line painted apart from the rest.
#[derive(Clone, Debug, PartialEq)]
pub struct TextMark {
    /// The bytes of the line it covers.
    pub range: Range<usize>,
    /// Its color.
    pub color: Color,
    /// How it is painted.
    pub paint: MarkPaint,
}

impl TextMark {
    /// Paints the bytes `range` in `color` the way `paint` says.
    pub fn new(range: Range<usize>, color: Color, paint: MarkPaint) -> Self {
        Self {
            range,
            color,
            paint,
        }
    }
}

/// How the carets of a [`marked_text`] line are drawn, how tall the line is, and who is told
/// its shaped layout.
#[derive(Clone, Default)]
pub struct MarkedTextOptions {
    /// The shape of its carets.
    pub caret_shape: CaretShape,
    /// Whether its carets are drawn now; the caller blinks them by turning this off and on.
    pub caret_visible: bool,
    /// How tall the line is, in logical pixels; the style's line height when `None`.
    pub line_height: Option<f32>,
    /// Called with the line's shaped layout once it is laid out, for a caller that maps points
    /// to bytes or keeps it for a selection.
    pub on_layout: Option<Rc<dyn Fn(Rc<TextLayout>)>>,
}

/// Whether `content` holds right-to-left letters, whose line the text node lays out against its
/// whole width, so its marks are placed from the line shaped alone instead, as it is drawn.
fn right_to_left(content: &str) -> bool {
    content
        .chars()
        .any(|letter| matches!(letter, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FEFF}'))
}

/// Draws a caret at byte `byte` of `layout`, which was drawn with its top-left corner at
/// (`x`, `y`), shaped as `shape` says, in `color`.
pub fn draw_text_caret(
    canvas: &mut Canvas,
    layout: &TextLayout,
    (x, y): (f32, f32),
    byte: usize,
    shape: CaretShape,
    color: Color,
) {
    let at = layout.caret(byte, Affinity::Downstream);
    let next = layout
        .text()
        .get(byte..)
        .and_then(|rest| rest.chars().next())
        .map(|character| byte + character.len_utf8());
    let edge = next
        .map(|next| layout.caret(next, Affinity::Upstream).x)
        .filter(|edge| (edge - at.x).abs() > 0.5);
    let (left, cell) = edge.map_or((at.x, at.height * 0.45), |edge| {
        (at.x.min(edge), (edge - at.x).abs())
    });
    match shape {
        CaretShape::Bar => canvas.rect(x + at.x, y + at.y, CARET_WIDTH, at.height, color),
        CaretShape::Block => {
            let faded = Color {
                a: (f32::from(color.a) * BLOCK_CARET_ALPHA) as u8,
                ..color
            };
            canvas.rect(x + left, y + at.y, cell, at.height, faded);
        }
        CaretShape::Underline => {
            canvas.rect(x + left, y + at.y + at.height - 2.0, cell, 2.0, color)
        }
    }
}

/// Draws a line under the bytes `range` of `layout`, which was drawn with its top-left corner
/// at (`x`, `y`) in type `size` pixels tall: straight, or wavy as a spelling mistake is marked.
pub fn draw_text_underline(
    canvas: &mut Canvas,
    layout: &TextLayout,
    (x, y): (f32, f32),
    range: Range<usize>,
    size: f32,
    wavy: bool,
    color: Color,
) {
    for rect in layout.selection_rects(range.start, range.end) {
        let line = layout.line_of_offset(range.start, Affinity::Downstream);
        let baseline = layout
            .lines()
            .get(line)
            .map_or(rect.size.height, |held| held.baseline - held.top);
        let left = x + rect.origin.x;
        let right = left + rect.size.width;
        let top = y + rect.origin.y + baseline;
        if !wavy {
            let thickness = (size / 14.0).max(1.0);
            canvas.rect(left, top + size * 0.12, right - left, thickness, color);
            continue;
        }
        let right = right.max(left + WAVE_STEP * 2.0);
        let mut from = left;
        let mut step = 0;
        while from < right {
            let lifted = if step % 2 == 0 { 0.0 } else { 1.0 };
            canvas.rect(
                from,
                top + 2.0 + lifted,
                WAVE_STEP.min(right - from),
                1.0,
                color,
            );
            from += WAVE_STEP;
            step += 1;
        }
    }
}

/// The spans of `content` with the stretches `marks` ink in their colors.
fn inked_spans(content: &str, marks: &[TextMark]) -> Vec<TextSpan> {
    let mut inked: Vec<&TextMark> = marks
        .iter()
        .filter(|mark| mark.paint == MarkPaint::Ink)
        .collect();
    inked.sort_by_key(|mark| mark.range.start);
    let mut spans = Vec::new();
    let mut last = 0;
    for mark in &inked {
        let start = mark.range.start.clamp(last, content.len());
        let end = mark.range.end.clamp(start, content.len());
        if start > last {
            spans.push(TextSpan::new(&content[last..start]));
        }
        if end > start {
            spans.push(TextSpan::new(&content[start..end]).color(mark.color));
        }
        last = end;
    }
    if last < content.len() || spans.is_empty() {
        spans.push(TextSpan::new(&content[last..]));
    }
    spans
}

/// Emits one line of `content` set in `style` with `marks` painted over and under it.
///
/// Inked stretches become the spans of one rich text, shaped once; fills and guides are drawn
/// behind it, underlines, drop points and carets over it, from the layout the text reports
/// when it is laid out. The line is as tall as `options` says.
#[track_caller]
pub fn marked_text(
    modifier: Modifier,
    content: &str,
    style: TextStyle,
    marks: Vec<TextMark>,
    options: MarkedTextOptions,
) {
    let spans = inked_spans(content, &marks);
    let space = layout_text(" ", &style, None).size().width;
    let size = style.font_size;
    let height = options.line_height.unwrap_or(style.line_height);
    let unbroken = right_to_left(content).then(|| Rc::new(layout_spans(&spans, &style, None)));
    if let (Some(layout), Some(told)) = (&unbroken, &options.on_layout) {
        told(layout.clone());
    }
    let shaped: Rc<RefCell<Option<Rc<TextLayout>>>> = Rc::new(RefCell::new(unbroken.clone()));
    let (stored, behind, over) = (shaped.clone(), shaped.clone(), shaped);
    let marks = Rc::new(marks);
    let (under, above) = (marks.clone(), marks);
    let told = options.on_layout.clone();
    let (shape, visible) = (options.caret_shape, options.caret_visible);
    let modifier = modifier
        .on_text_layout(move |layout| {
            if unbroken.is_none() {
                *stored.borrow_mut() = Some(layout.clone());
                if let Some(told) = &told {
                    told(layout);
                }
            }
        })
        .draw_behind(move |canvas| {
            let Some(layout) = behind.borrow().clone() else {
                return;
            };
            for mark in under.iter() {
                match mark.paint {
                    MarkPaint::Fill => canvas.selection(
                        0.0,
                        0.0,
                        &layout,
                        mark.range.start,
                        mark.range.end,
                        mark.color,
                    ),
                    MarkPaint::Guide => canvas.rect(
                        space * mark.range.start as f32,
                        0.0,
                        1.0,
                        height,
                        mark.color,
                    ),
                    _ => {}
                }
            }
        })
        .draw_with_content(move |canvas| {
            canvas.draw_content();
            let Some(layout) = over.borrow().clone() else {
                return;
            };
            for mark in above.iter() {
                let range = mark.range.clone();
                match mark.paint {
                    MarkPaint::Underline => draw_text_underline(
                        canvas,
                        &layout,
                        (0.0, 0.0),
                        range,
                        size,
                        false,
                        mark.color,
                    ),
                    MarkPaint::Wave => draw_text_underline(
                        canvas,
                        &layout,
                        (0.0, 0.0),
                        range,
                        size,
                        true,
                        mark.color,
                    ),
                    MarkPaint::Drop => {
                        let at = layout.caret(range.start, Affinity::Downstream);
                        canvas.rect(at.x, at.y, CARET_WIDTH, at.height, mark.color);
                    }
                    MarkPaint::Caret if visible => {
                        draw_text_caret(canvas, &layout, (0.0, 0.0), range.start, shape, mark.color)
                    }
                    _ => {}
                }
            }
        });
    LOCAL_TEXT_STYLE.provide(style, || rich_text_with(modifier.height(height), spans));
}

#[cfg(test)]
mod tests {
    use rustle_graphics::Color;

    use super::{MarkPaint, TextMark, inked_spans};

    /// Inked stretches split the line into spans in their colors, the rest left plain.
    #[test]
    fn ink_splits_the_line_into_spans() {
        let red = Color::rgb(255, 0, 0);
        let marks = vec![
            TextMark::new(4..7, red, MarkPaint::Ink),
            TextMark::new(0..3, red, MarkPaint::Fill),
        ];
        let spans = inked_spans("let foo = 1;", &marks);
        let texts: Vec<&str> = spans.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(texts, vec!["let ", "foo", " = 1;"]);
        assert_eq!(spans[1].color, Some(red));
    }
}
