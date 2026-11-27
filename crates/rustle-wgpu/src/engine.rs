//! The text engine: glyph-level layout and the font database, shared with the
//! renderer's font system and reachable from `rustle-text`'s functions.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use glyphon::{Buffer, FontSystem};
use rustle_graphics::StyledSpan;
use rustle_text::{Cluster, LineLayout, TextEngine, TextLayout, TextStyle};

use crate::text::{Content, Setting, with_shaped};

/// Shares the renderer's font system, so fonts registered through it are the
/// ones the renderer draws with.
pub(super) struct Engine {
    pub(super) font_system: Rc<RefCell<FontSystem>>,
    /// Counts font registrations, so caches of shaped text know to start over.
    pub(super) generation: Rc<Cell<u64>>,
}

impl TextEngine for Engine {
    fn layout(
        &self,
        spans: &[StyledSpan],
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> TextLayout {
        layout_spans(&mut self.font_system.borrow_mut(), spans, style, max_width)
    }

    fn font_families(&self) -> Vec<String> {
        let font_system = self.font_system.borrow();
        let names: BTreeSet<String> = font_system
            .db()
            .faces()
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            .collect();
        names.into_iter().collect()
    }

    fn monospace_font_families(&self) -> Vec<String> {
        let font_system = self.font_system.borrow();
        let names: BTreeSet<String> = font_system
            .db()
            .faces()
            .filter(|face| face.monospaced)
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            .collect();
        names.into_iter().collect()
    }

    fn register_font(&self, data: Vec<u8>) -> Vec<String> {
        let mut font_system = self.font_system.borrow_mut();
        let known: BTreeSet<_> = font_system.db().faces().map(|face| face.id).collect();
        font_system.db_mut().load_font_data(data);
        let added: BTreeSet<String> = font_system
            .db()
            .faces()
            .filter(|face| !known.contains(&face.id))
            .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
            .collect();
        self.generation.set(self.generation.get() + 1);
        hand_fonts_to_svg(&font_system);
        added.into_iter().collect()
    }
}

/// Hands every font `font_system` holds to SVG documents, with the families its generic names
/// stand for, so the text of an [`SvgPainter`](rustle_graphics::SvgPainter) is set with the
/// fonts the rest of the interface uses.
pub(crate) fn hand_fonts_to_svg(font_system: &FontSystem) {
    use glyphon::fontdb::{Family, Source};
    let database = font_system.db();
    let sources = database
        .faces()
        .map(|face| match &face.source {
            Source::Binary(data) => {
                rustle_graphics::SvgFontSource::Data(std::sync::Arc::clone(data))
            }
            Source::File(path) | Source::SharedFile(path, _) => {
                rustle_graphics::SvgFontSource::File(path.clone())
            }
        })
        .collect();
    let family = |generic: Family<'_>| {
        let name = database.family_name(&generic);
        (!name.is_empty()).then(|| name.to_string())
    };
    rustle_graphics::set_svg_fonts(
        sources,
        family(Family::SansSerif),
        family(Family::Serif),
        family(Family::Monospace),
    );
}

/// `spans` shaped and broken into lines at `width`, as a [`TextLayout`].
pub(super) fn layout_spans(
    font_system: &mut FontSystem,
    spans: &[StyledSpan],
    style: &TextStyle,
    width: Option<f32>,
) -> TextLayout {
    let text: String = spans.iter().map(|span| span.text.as_str()).collect();
    let mut end = 0;
    let span_ends: Vec<usize> = spans
        .iter()
        .map(|span| {
            end += span.text.len();
            end
        })
        .collect();
    let lines = with_shaped(
        font_system,
        Setting {
            content: Content::Rich(spans),
            font_size: style.font_size,
            line_height: style.line_height,
            align: style.align,
            max_lines: style.max_lines,
            wrap: style.wrap,
        },
        width,
        |buffer| lines_of(buffer, &text, &span_ends, style),
    );
    TextLayout::new(text, spans.to_vec(), style.font_size, lines)
}

/// Where the text of `run` starts in the whole text, its paragraph starting
/// at `first`; the space a line wrapped at is left at the end of the line
/// before, so the next line starts at its first glyph.
fn next_start(run: &glyphon::LayoutRun<'_>, first: usize) -> Option<usize> {
    run.glyphs.iter().map(|glyph| first + glyph.start).min()
}

/// The index of the span holding byte `offset` of the text, given where each span ends.
fn span_at(span_ends: &[usize], offset: usize) -> usize {
    span_ends
        .partition_point(|end| *end <= offset)
        .min(span_ends.len().saturating_sub(1))
}

/// The visual lines of a shaped buffer, with offsets into the whole text and the span each
/// cluster is in, found by where each span ends in `span_ends`.
fn lines_of(
    buffer: &Buffer,
    text: &str,
    span_ends: &[usize],
    style: &TextStyle,
) -> Vec<LineLayout> {
    let mut starts = Vec::with_capacity(buffer.lines.len());
    let mut total = 0;
    for line in &buffer.lines {
        starts.push(total);
        total += line.text().len() + line.ending().as_str().len();
    }
    let runs: Vec<_> = buffer.layout_runs().collect();
    let mut lines: Vec<LineLayout> = Vec::new();
    let mut bottom = 0.0_f32;
    let mut next_run = 0;
    for (paragraph, line) in buffer.lines.iter().enumerate() {
        let first = starts[paragraph];
        let length = line.text().len();
        let from = next_run
            + runs[next_run..]
                .iter()
                .take_while(|run| run.line_i < paragraph)
                .count();
        next_run = from
            + runs[from..]
                .iter()
                .take_while(|run| run.line_i == paragraph)
                .count();
        let mine: Vec<_> = runs[from..next_run].iter().collect();
        let goes_on = crate::paragraphs::continues(line);
        if mine.is_empty() {
            // An empty line (or one cut off by an ellipsis) has no run; an empty one still takes room.
            if style.max_lines.is_none() {
                lines.push(LineLayout {
                    start: first,
                    end: first + length,
                    top: bottom,
                    height: style.line_height,
                    baseline: bottom + style.line_height * 0.8,
                    width: 0.0,
                    rtl: false,
                    hard_break: true,
                    clusters: Vec::new(),
                });
                bottom += style.line_height;
            }
            continue;
        }
        let mut start = first;
        for (index, run) in mine.iter().enumerate() {
            let mut clusters: Vec<Cluster> = Vec::new();
            for glyph in crate::paragraphs::line_glyphs(buffer, run) {
                let (cluster_start, cluster_end) = (first + glyph.start, first + glyph.end);
                let right = glyph.x + glyph.w;
                match clusters.last_mut() {
                    Some(last) if last.start == cluster_start && last.end == cluster_end => {
                        let edge = (last.x + last.width).max(right);
                        last.x = last.x.min(glyph.x);
                        last.width = edge - last.x;
                    }
                    _ => clusters.push(Cluster {
                        start: cluster_start,
                        end: cluster_end,
                        x: glyph.x,
                        width: glyph.w,
                        rtl: glyph.level.is_rtl(),
                        span: span_at(span_ends, cluster_start),
                    }),
                }
            }
            let last = index + 1 == mine.len();
            let end = if last {
                first + length
            } else {
                next_start(mine[index + 1], first)
                    .unwrap_or_else(|| clusters.iter().map(|c| c.end).max().unwrap_or(start))
            };
            lines.push(LineLayout {
                start,
                end: end.max(start),
                top: run.line_top,
                height: run.line_height,
                baseline: run.line_y,
                width: crate::paragraphs::line_width(buffer, run),
                rtl: run.rtl,
                hard_break: last && !goes_on,
                clusters,
            });
            start = end;
            bottom = run.line_top + run.line_height;
        }
    }
    // A line break at the very end leaves an empty last line the shaper does not report.
    if text.ends_with('\n') && style.max_lines.is_none() {
        lines.push(LineLayout {
            start: text.len(),
            end: text.len(),
            top: bottom,
            height: style.line_height,
            baseline: bottom + style.line_height * 0.8,
            width: 0.0,
            rtl: false,
            hard_break: true,
            clusters: Vec::new(),
        });
    }
    lines
}

#[cfg(test)]
mod tests {
    use rustle_graphics::{Color, FontFamily};
    use rustle_text::{Affinity, TextSpan};

    use super::*;

    fn fonts() -> FontSystem {
        FontSystem::new()
    }

    fn layout(text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
        let spans = [TextSpan::new(text).resolve(style)];
        layout_spans(&mut fonts(), &spans, style, width)
    }

    /// A paragraph streamed in a few words at a time, with bold words, a link and its
    /// newest characters fading in, lays out the same, line for line and cluster for
    /// cluster, as when it is shaped whole each time, at two widths in turn as measuring
    /// and drawing do.
    #[test]
    fn a_growing_paragraph_lays_out_as_when_shaped_whole() {
        let style = TextStyle::default();
        let vocabulary = [
            "streaming",
            "text",
            "grows",
            "by",
            "a",
            "few",
            "words",
            "every",
            "frame",
            "and",
            "its",
            "newest",
            "characters",
            "fade",
            "in",
            "while",
            "older",
            "lines",
            "stay",
        ];
        let words: Vec<&str> = (0..400)
            .map(|index| vocabulary[(index * 7 + index / 3) % vocabulary.len()])
            .collect();
        let mut fonts = fonts();
        let cut_before = crate::paragraphs::paragraphs_cut();
        for step in (40..400).step_by(9) {
            let shown = words[..step].join(" ");
            let fade = shown.len() - 24;
            let mut spans = Vec::new();
            let mut at = 0;
            for (index, word) in shown[..fade].split_inclusive(' ').enumerate() {
                let mut span = TextSpan::new(word).resolve(&style);
                if index % 11 == 3 {
                    span.font_weight = 700;
                }
                if index % 29 == 5 {
                    span.color = Color::rgb(90, 140, 250);
                }
                spans.push(span);
                at += word.len();
            }
            for (index, piece) in shown.as_bytes()[at..].chunks(6).enumerate() {
                let mut span =
                    TextSpan::new(std::str::from_utf8(piece).expect("ascii")).resolve(&style);
                span.color.a = 250 - ((index + step % 7) * 30 % 240) as u8;
                spans.push(span);
            }
            for width in [320.0, 321.0] {
                let grown = layout_spans(&mut fonts, &spans, &style, Some(width));
                let whole = crate::paragraphs::shaped_whole(|| {
                    layout_spans(&mut fonts, &spans, &style, Some(width))
                });
                assert_eq!(
                    grown.line_count(),
                    whole.line_count(),
                    "{step} words at {width}"
                );
                for (line, (grown, whole)) in grown.lines().iter().zip(whole.lines()).enumerate() {
                    assert_eq!(grown, whole, "line {line} of {step} words at {width}");
                }
            }
        }
        assert!(crate::paragraphs::paragraphs_cut() > cut_before + 40);
    }

    #[test]
    fn wrapping_breaks_lines_inside_the_width() {
        let style = TextStyle::default();
        let text = "the quick brown fox jumps over the lazy dog and keeps running far away";
        let wrapped = layout(text, &style, Some(120.0));
        assert!(wrapped.line_count() > 2);
        let mut next = 0;
        for line in wrapped.lines() {
            assert_eq!(line.start, next, "lines are contiguous");
            assert!(line.width <= 120.5, "line of {} is too wide", line.width);
            next = line.end;
        }
        assert_eq!(next, text.len());
        assert!(wrapped.lines().last().is_some_and(|l| l.hard_break));
        assert!(wrapped.lines().iter().rev().skip(1).all(|l| !l.hard_break));
        let flat = layout(text, &style, None);
        assert_eq!(flat.line_count(), 1);
        let none = layout(
            text,
            &style.wrap(rustle_graphics::TextWrap::None),
            Some(120.0),
        );
        assert_eq!(none.line_count(), 1);
    }

    #[test]
    fn a_wrapped_line_keeps_the_space_it_wrapped_at() {
        let style = TextStyle::default();
        let text = "the quick brown fox jumps over the lazy dog and keeps running far away";
        let wrapped = layout(text, &style, Some(120.0));
        for line in &wrapped.lines()[1..] {
            assert!(
                !text[line.start..].starts_with(' '),
                "line at {} starts with a space",
                line.start
            );
        }
    }

    #[test]
    fn a_long_word_breaks_by_character() {
        let style = TextStyle::default();
        let layout = layout(&"w".repeat(60), &style, Some(100.0));
        assert!(layout.line_count() > 1);
        assert!(layout.lines().iter().all(|l| l.width <= 100.5));
    }

    #[test]
    fn line_breaks_make_empty_lines() {
        let style = TextStyle::default();
        assert_eq!(layout("a\n", &style, None).line_count(), 2);
        let layout = layout("a\n\nb", &style, None);
        assert_eq!(layout.line_count(), 3);
        assert_eq!((layout.lines()[1].start, layout.lines()[1].end), (2, 2));
        assert_eq!(layout.lines()[2].start, 3);
        assert_eq!(layout.lines()[2].top, 2.0 * style.line_height);
    }

    #[test]
    fn hit_testing_inverts_carets() {
        let style = TextStyle::default();
        let text = "hello wrapped world, this is a test of carets\nsecond line";
        let layout = layout(text, &style, Some(150.0));
        for (offset, _) in text.char_indices() {
            let caret = layout.caret(offset, Affinity::Downstream);
            let point = rustle_geometry::Offset::new(caret.x + 0.01, caret.y + caret.height / 2.0);
            // A space left at a wrap can take no room, so only the place has to agree.
            let hit = layout.caret(layout.hit_test(point), Affinity::Downstream);
            assert!(
                (hit.x - caret.x).abs() < 0.01 && hit.line == caret.line,
                "offset {offset}"
            );
        }
        let positions = layout.char_positions();
        assert_eq!(positions.len(), text.chars().count() + 1);
    }

    #[test]
    fn right_to_left_text_runs_backwards() {
        let style = TextStyle::default();
        let hebrew = "\u{5e9}\u{5dc}\u{5d5}\u{5dd} \u{5e2}\u{5d5}\u{5dc}\u{5dd}";
        let text = format!("abc {hebrew} def");
        let layout = layout(&text, &style, None);
        let clusters = &layout.lines()[0].clusters;
        let rtl: Vec<_> = clusters.iter().filter(|c| c.rtl).collect();
        assert!(!rtl.is_empty(), "the Hebrew run is right to left");
        assert!(rtl.windows(2).all(|pair| pair[0].start > pair[1].start));
        assert!(clusters.iter().any(|c| !c.rtl));
        // A caret before the first Hebrew letter is on the right of the run.
        let first = text.find('\u{5e9}').unwrap_or(0);
        let last = text.find('\u{5dd}').unwrap_or(0);
        let before_first = layout.caret(first, Affinity::Downstream).x;
        let after_first = layout
            .caret(first + '\u{5e9}'.len_utf8(), Affinity::Downstream)
            .x;
        assert!(before_first > after_first);
        assert!(layout.caret(last, Affinity::Downstream).x < before_first);
        // A selection across a mixed run is still within the line.
        let rects = layout.selection_rects(2, text.len() - 2);
        assert!(!rects.is_empty());
        // A paragraph that starts right to left has its lines aligned to the right.
        let paragraph = super::tests::layout(hebrew, &style, Some(300.0));
        assert!(paragraph.lines()[0].rtl);
    }

    #[test]
    fn spans_change_size_spacing_and_line_height() {
        let style = TextStyle::default();
        let plain = [TextSpan::new("abcd efgh").resolve(&style)];
        let spaced = [TextSpan::new("abcd efgh")
            .letter_spacing(5.0)
            .resolve(&style)];
        let big = [
            TextSpan::new("abcd ").resolve(&style),
            TextSpan::new("efgh").font_size(40.0).resolve(&style),
        ];
        let mut fs = fonts();
        let a = layout_spans(&mut fs, &plain, &style, None);
        let b = layout_spans(&mut fs, &spaced, &style, None);
        let c = layout_spans(&mut fs, &big, &style, None);
        assert!(
            b.size().width > a.size().width + 40.0,
            "9 characters at 5 px"
        );
        assert!(c.size().height > a.size().height * 1.5);
        assert!(c.lines()[0].clusters.iter().any(|cl| cl.span == 1));
    }

    #[test]
    fn ligatures_can_be_turned_off() {
        let mut fs = fonts();
        if !fs
            .db()
            .faces()
            .any(|f| f.families.iter().any(|(n, _)| n == "DejaVu Sans"))
        {
            return;
        }
        let style = TextStyle::default().font_family(FontFamily::Named("DejaVu Sans"));
        let on = layout_spans(
            &mut fs,
            &[TextSpan::new("ffi fl").resolve(&style)],
            &style,
            None,
        );
        let off_style = style.ligatures(false);
        let off = layout_spans(
            &mut fs,
            &[TextSpan::new("ffi fl").resolve(&off_style)],
            &off_style,
            None,
        );
        assert_ne!(on.size().width, off.size().width);
    }

    #[test]
    fn spans_know_their_background_and_underline() {
        let style = TextStyle::default();
        let spans = [
            TextSpan::new("plain ").resolve(&style),
            TextSpan::new("marked")
                .underline()
                .background(Color::rgb(255, 0, 0))
                .resolve(&style),
        ];
        let layout = layout_spans(&mut fonts(), &spans, &style, None);
        let keep = |color| color;
        let origin = rustle_geometry::Offset::default();
        assert_eq!(layout.background_commands(origin, &keep).len(), 1);
        assert_eq!(
            layout
                .decoration_commands(origin, false, false, &keep)
                .len(),
            1
        );
    }

    #[test]
    fn fonts_can_be_listed_and_registered() {
        let engine = Engine {
            font_system: Rc::new(RefCell::new(fonts())),
            generation: Rc::default(),
        };
        let families = engine.font_families();
        assert!(
            families.windows(2).all(|pair| pair[0] < pair[1]),
            "sorted, no repeats"
        );
        let monospace = engine.monospace_font_families();
        assert!(
            monospace.iter().all(|name| families.contains(name)),
            "monospaced families are among all families"
        );
        let Ok(data) = std::fs::read("/usr/share/fonts/TTF/DejaVuSans.ttf") else {
            return;
        };
        let added = engine.register_font(data);
        assert!(added.iter().any(|name| name == "DejaVu Sans"));
        assert_eq!(engine.generation.get(), 1);
    }
}
