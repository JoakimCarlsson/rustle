//! Glyph-level text layout: line breaks, per-line metrics, caret positions and hit testing.
//!
//! A [`TextLayout`] is plain data produced by the renderer's text engine (see
//! [`crate::layout_text`]). Offsets into the text are UTF-8 byte offsets, always on
//! character boundaries. Coordinates are logical pixels from the top-left corner of
//! the text box.

use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Color, DrawCommand, StyledSpan};

/// Which side of a soft line break an offset belongs to. The end of a wrapped
/// line and the start of the next one are the same offset; the affinity says which
/// of the two places a caret is drawn at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Affinity {
    /// The caret sits with the character after it: at the start of the next line.
    #[default]
    Downstream,
    /// The caret sits with the character before it: at the end of the previous line.
    Upstream,
}

/// One shaped cluster: the glyph or glyphs that stand for one or more characters.
/// A ligature such as `fi` is one cluster of two characters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cluster {
    /// The first byte of the characters this cluster stands for.
    pub start: usize,
    /// One past the last byte of those characters.
    pub end: usize,
    /// Left edge from the left of the text box, after alignment.
    pub x: f32,
    pub width: f32,
    /// Whether the cluster is part of a right-to-left run.
    pub rtl: bool,
    /// Which span of the text the cluster belongs to.
    pub span: usize,
}

/// One visual line: a line break in the text, or a place the text wrapped.
/// Clusters are in visual order, left to right.
#[derive(Debug, Clone, PartialEq)]
pub struct LineLayout {
    /// The first byte on the line.
    pub start: usize,
    /// One past the last byte on the line, not counting the line break that ends it.
    pub end: usize,
    /// Distance from the top of the text box to the top of the line.
    pub top: f32,
    pub height: f32,
    /// Distance from the top of the text box to the baseline.
    pub baseline: f32,
    /// The width of what is on the line, including a trailing space left at a wrap.
    pub width: f32,
    /// Whether the line's paragraph runs right to left.
    pub rtl: bool,
    /// Whether the line ends at a line break or the end of the text, rather than where the text wrapped.
    pub hard_break: bool,
    pub clusters: Vec<Cluster>,
}

/// Where a caret is drawn: a vertical bar from (`x`, `y`) down `height`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Caret {
    pub x: f32,
    pub y: f32,
    pub height: f32,
    /// The visual line the caret is on.
    pub line: usize,
}

/// Text that has been shaped and broken into lines.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    text: Rc<str>,
    spans: Rc<Vec<StyledSpan>>,
    base_font_size: f32,
    lines: Rc<[LineLayout]>,
    size: Size,
}

fn boundary_after(text: &str, start: usize, chars: usize) -> usize {
    text[start..]
        .char_indices()
        .nth(chars)
        .map_or(text.len(), |(at, _)| start + at)
}

impl Cluster {
    fn char_count(&self, text: &str) -> usize {
        text.get(self.start..self.end)
            .map_or(1, |s| s.chars().count())
            .max(1)
    }

    /// The x of the boundary at `offset`, which must lie from `start` to `end`.
    fn x_at(&self, text: &str, offset: usize) -> f32 {
        let total = self.char_count(text) as f32;
        let before = text
            .get(self.start..offset.clamp(self.start, self.end))
            .map_or(0, |s| s.chars().count()) as f32;
        let fraction = before / total;
        if self.rtl {
            self.x + self.width * (1.0 - fraction)
        } else {
            self.x + self.width * fraction
        }
    }

    /// The offset at the visual left (`left`) or right edge.
    fn edge_offset(&self, left: bool) -> usize {
        if left != self.rtl {
            self.start
        } else {
            self.end
        }
    }
}

impl LineLayout {
    /// The x of the caret at `offset`, which must lie from `start` to `end`.
    fn x_at(&self, text: &str, offset: usize) -> f32 {
        if let Some(cluster) = self
            .clusters
            .iter()
            .find(|c| offset >= c.start && offset < c.end)
        {
            return cluster.x_at(text, offset);
        }
        match self
            .clusters
            .iter()
            .filter(|c| c.end <= offset)
            .max_by_key(|c| c.end)
        {
            Some(cluster) if cluster.rtl => cluster.x,
            Some(cluster) => cluster.x + cluster.width,
            None => 0.0,
        }
    }

    /// The offset nearest to `x` on this line.
    fn offset_at_x(&self, text: &str, x: f32) -> usize {
        let Some(last) = self.clusters.last() else {
            return self.start;
        };
        for cluster in &self.clusters {
            if x < cluster.x + cluster.width {
                if x <= cluster.x {
                    return cluster.edge_offset(true);
                }
                let chars = cluster.char_count(text);
                let across = ((x - cluster.x) / cluster.width.max(0.001) * chars as f32)
                    .round()
                    .clamp(0.0, chars as f32) as usize;
                let logical = if cluster.rtl { chars - across } else { across };
                return boundary_after(text, cluster.start, logical);
            }
        }
        last.edge_offset(false)
    }
}

impl TextLayout {
    /// Builds a layout from lines already placed. `spans` are the runs the text was
    /// made of, which clusters refer to by index, and `base_font_size` the size runs
    /// without their own are set in. The engine of a renderer calls this.
    pub fn new(
        text: impl Into<Rc<str>>,
        spans: Vec<StyledSpan>,
        base_font_size: f32,
        lines: Vec<LineLayout>,
    ) -> Self {
        let width = lines.iter().fold(0.0_f32, |w, line| w.max(line.width));
        let height = lines.last().map_or(0.0, |line| line.top + line.height);
        Self {
            text: text.into(),
            spans: Rc::new(spans),
            base_font_size,
            lines: lines.into(),
            size: Size::new(width, height),
        }
    }

    /// A layout of nothing, which is what the text functions return when no renderer
    /// has been created yet.
    pub fn empty() -> Self {
        Self::new("", Vec::new(), 0.0, Vec::new())
    }

    /// The text that was laid out, all spans joined.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The width of the widest line and the height of all lines together.
    pub fn size(&self) -> Size {
        self.size
    }

    /// The visual lines, top to bottom. Text with a trailing line break ends in an empty line.
    pub fn lines(&self) -> &[LineLayout] {
        &self.lines
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// The text of the visual line `line`, without its line break.
    pub fn line_text(&self, line: usize) -> &str {
        self.lines
            .get(line)
            .and_then(|l| self.text.get(l.start..l.end))
            .unwrap_or("")
    }

    /// The visual line a caret at `offset` is on.
    pub fn line_of_offset(&self, offset: usize, affinity: Affinity) -> usize {
        let offset = offset.min(self.text.len());
        let mut found = None;
        for (index, line) in self.lines.iter().enumerate() {
            if offset >= line.start && offset <= line.end {
                found = Some(index);
                if affinity == Affinity::Upstream {
                    break;
                }
            } else if found.is_some() {
                break;
            }
        }
        found.unwrap_or(self.lines.len().saturating_sub(1))
    }

    /// The visual line at height `y`: the first for anything above the text, the last for anything below.
    pub fn line_at_y(&self, y: f32) -> usize {
        self.lines
            .iter()
            .position(|line| y < line.top + line.height)
            .unwrap_or(self.lines.len().saturating_sub(1))
    }

    /// Where a caret at `offset` is drawn.
    pub fn caret(&self, offset: usize, affinity: Affinity) -> Caret {
        let offset = offset.min(self.text.len());
        let index = self.line_of_offset(offset, affinity);
        match self.lines.get(index) {
            Some(line) => Caret {
                x: line.x_at(&self.text, offset.clamp(line.start, line.end)),
                y: line.top,
                height: line.height,
                line: index,
            },
            None => Caret {
                x: 0.0,
                y: 0.0,
                height: self.base_font_size * 1.4,
                line: 0,
            },
        }
    }

    /// The offset nearest to `x` on visual line `line`, and which side of a wrap it is on.
    pub fn offset_on_line(&self, line: usize, x: f32) -> (usize, Affinity) {
        let Some(l) = self.lines.get(line) else {
            return (0, Affinity::Downstream);
        };
        let offset = l.offset_at_x(&self.text, x).clamp(l.start, l.end);
        (offset, self.affinity_at_end(l, offset))
    }

    fn affinity_at_end(&self, line: &LineLayout, offset: usize) -> Affinity {
        if offset == line.end && !line.hard_break {
            Affinity::Upstream
        } else {
            Affinity::Downstream
        }
    }

    /// The offset of the character boundary nearest to `point`, and its affinity.
    pub fn hit_test_with_affinity(&self, point: Offset) -> (usize, Affinity) {
        if self.lines.is_empty() {
            return (0, Affinity::Downstream);
        }
        self.offset_on_line(self.line_at_y(point.y), point.x)
    }

    /// The offset of the character boundary nearest to `point`.
    pub fn hit_test(&self, point: Offset) -> usize {
        self.hit_test_with_affinity(point).0
    }

    /// Where the caret goes moving `delta` visual lines (negative is up) from `offset`,
    /// keeping to `x` (the caret's own x if `None`). `None` when there is no such line.
    pub fn move_vertically(
        &self,
        offset: usize,
        affinity: Affinity,
        delta: isize,
        x: Option<f32>,
    ) -> Option<(usize, Affinity)> {
        let caret = self.caret(offset, affinity);
        let target = caret.line.checked_add_signed(delta)?;
        if target >= self.lines.len() {
            return None;
        }
        Some(self.offset_on_line(target, x.unwrap_or(caret.x)))
    }

    /// The start of the visual line `offset` is on, as a caret position.
    pub fn line_start(&self, offset: usize, affinity: Affinity) -> (usize, Affinity) {
        let Some(line) = self.lines.get(self.line_of_offset(offset, affinity)) else {
            return (0, Affinity::Downstream);
        };
        (line.start, Affinity::Downstream)
    }

    /// The end of the visual line `offset` is on, as a caret position. On a wrapped
    /// line this is the end of the line, not the start of the next one.
    pub fn line_end(&self, offset: usize, affinity: Affinity) -> (usize, Affinity) {
        let Some(line) = self.lines.get(self.line_of_offset(offset, affinity)) else {
            return (0, Affinity::Downstream);
        };
        (line.end, self.affinity_at_end(line, line.end))
    }

    /// The position of the caret at every character boundary of the text, as an x and a
    /// visual line, one more than there are characters. Boundaries at a wrap are
    /// reported at the start of the next line.
    pub fn char_positions(&self) -> Vec<(f32, usize)> {
        let mut positions = Vec::with_capacity(self.text.len() + 1);
        for (index, line) in self.lines.iter().enumerate() {
            let at = |offset: usize| line.x_at(&self.text, offset);
            let own = self.text.get(line.start..line.end).unwrap_or("");
            positions.extend(own.char_indices().map(|(i, _)| (at(line.start + i), index)));
            match self.lines.get(index + 1) {
                // A wrapped line's end is the next line's start.
                Some(next) if next.start == line.end => {}
                next => {
                    positions.push((at(line.end), index));
                    // Boundaries inside a line break of more than one character.
                    if let Some(next) = next {
                        let break_chars = self
                            .text
                            .get(line.end..next.start)
                            .map_or(1, |s| s.chars().count());
                        for _ in 1..break_chars {
                            positions.push((at(line.end), index));
                        }
                    }
                }
            }
        }
        if positions.is_empty() {
            positions.push((0.0, 0));
        }
        positions
    }

    /// The rectangles that highlight the text from `start` to `end`: one or more per
    /// line (a right-to-left run inside left-to-right text makes separate ones), plus
    /// a little extra width where a line break is selected.
    pub fn selection_rects(&self, start: usize, end: usize) -> Vec<Rect> {
        let (start, end) = (start.min(end), start.max(end).min(self.text.len()));
        let mut rects = Vec::new();
        if start == end {
            return rects;
        }
        for (index, line) in self.lines.iter().enumerate() {
            if end < line.start || start > line.end {
                continue;
            }
            let mut spans: Vec<(f32, f32)> = Vec::new();
            for cluster in &line.clusters {
                let (from, to) = (start.max(cluster.start), end.min(cluster.end));
                if from >= to {
                    continue;
                }
                let (a, b) = (cluster.x_at(&self.text, from), cluster.x_at(&self.text, to));
                spans.push((a.min(b), a.max(b)));
            }
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut merged: Vec<(f32, f32)> = Vec::new();
            for (left, right) in spans {
                match merged.last_mut() {
                    Some(last) if left <= last.1 + 0.5 => last.1 = last.1.max(right),
                    _ => merged.push((left, right)),
                }
            }
            if line.hard_break
                && index + 1 < self.lines.len()
                && start <= line.end
                && end > line.end
            {
                let at = line.x_at(&self.text, line.end);
                let extra = line.height * 0.3;
                match merged.last_mut() {
                    Some(last) if (last.1 - at).abs() < 0.5 => last.1 += extra,
                    _ => merged.push((at, at + extra)),
                }
            }
            for (left, right) in merged {
                rects.push(Rect::new(
                    Offset::new(left, line.top),
                    Size::new(right - left, line.height),
                ));
            }
        }
        rects
    }

    fn span_size(&self, span: usize) -> f32 {
        self.spans
            .get(span)
            .and_then(|s| s.font_size)
            .unwrap_or(self.base_font_size)
    }

    /// Rectangles filling behind runs that have a background, for drawing before the text.
    /// `origin` is where the text box is; `fade` maps each color (for example to apply an alpha).
    pub fn background_commands(
        &self,
        origin: Offset,
        fade: &dyn Fn(Color) -> Color,
    ) -> Vec<DrawCommand> {
        let mut commands = Vec::new();
        for line in self.lines.iter() {
            self.runs(line, |span, left, right| {
                if let Some(color) = self.spans.get(span).and_then(|s| s.background) {
                    commands.push(rect_command(
                        origin.x + left,
                        origin.y + line.top,
                        right - left,
                        line.height,
                        fade(color),
                    ));
                }
            });
        }
        commands
    }

    /// Thin rectangles for the underlines and strikethroughs of runs that have them, for
    /// drawing over the text. Runs without their own settings use `underline` and `strikethrough`.
    pub fn decoration_commands(
        &self,
        origin: Offset,
        underline: bool,
        strikethrough: bool,
        fade: &dyn Fn(Color) -> Color,
    ) -> Vec<DrawCommand> {
        let mut commands = Vec::new();
        for line in self.lines.iter() {
            self.runs(line, |index, left, right| {
                let Some(span) = self.spans.get(index) else {
                    return;
                };
                let size = self.span_size(index);
                let thickness = (size / 14.0).max(1.0);
                let color = fade(span.color);
                if span.underline || underline {
                    commands.push(rect_command(
                        origin.x + left,
                        origin.y + line.baseline + size * 0.1,
                        right - left,
                        thickness,
                        color,
                    ));
                }
                if span.strikethrough || strikethrough {
                    commands.push(rect_command(
                        origin.x + left,
                        origin.y + line.baseline - size * 0.3,
                        right - left,
                        thickness,
                        color,
                    ));
                }
            });
        }
        commands
    }

    /// Calls `each` with the span and the left and right edge of each stretch of
    /// neighbouring clusters on `line` that belong to one span.
    fn runs(&self, line: &LineLayout, mut each: impl FnMut(usize, f32, f32)) {
        let mut current: Option<(usize, f32, f32)> = None;
        for cluster in &line.clusters {
            let right = cluster.x + cluster.width;
            current = match current {
                Some((span, left, edge)) if span == cluster.span && cluster.x <= edge + 0.5 => {
                    Some((span, left, right.max(edge)))
                }
                Some((span, left, edge)) => {
                    each(span, left, edge);
                    Some((cluster.span, cluster.x, right))
                }
                None => Some((cluster.span, cluster.x, right)),
            };
        }
        if let Some((span, left, edge)) = current {
            each(span, left, edge);
        }
    }
}

fn rect_command(x: f32, y: f32, width: f32, height: f32, color: Color) -> DrawCommand {
    DrawCommand::Rect {
        rect: Rect::new(Offset::new(x, y), Size::new(width, height)),
        color,
        corner_radius: 0.0,
        border: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "hello world" at 10 px per character, wrapped after "hello ", then a line
    /// break and "ab". Lines are 20 px tall.
    fn sample() -> TextLayout {
        let cluster = |start: usize, x: f32| Cluster {
            start,
            end: start + 1,
            x,
            width: 10.0,
            rtl: false,
            span: 0,
        };
        let line = |start: usize, end: usize, top: f32, hard: bool| LineLayout {
            start,
            end,
            top,
            height: 20.0,
            baseline: top + 15.0,
            width: (end - start) as f32 * 10.0,
            rtl: false,
            hard_break: hard,
            clusters: (start..end)
                .map(|i| cluster(i, (i - start) as f32 * 10.0))
                .collect(),
        };
        TextLayout::new(
            "hello world\nab",
            Vec::new(),
            14.0,
            vec![
                line(0, 6, 0.0, false),
                line(6, 11, 20.0, true),
                line(12, 14, 40.0, true),
            ],
        )
    }

    #[test]
    fn caret_at_a_wrap_depends_on_affinity() {
        let layout = sample();
        let down = layout.caret(6, Affinity::Downstream);
        assert_eq!((down.line, down.x, down.y), (1, 0.0, 20.0));
        let up = layout.caret(6, Affinity::Upstream);
        assert_eq!((up.line, up.x, up.y), (0, 60.0, 0.0));
    }

    #[test]
    fn hit_test_rounds_to_the_nearest_boundary() {
        let layout = sample();
        assert_eq!(layout.hit_test(Offset::new(14.0, 5.0)), 1);
        assert_eq!(layout.hit_test(Offset::new(16.0, 5.0)), 2);
        assert_eq!(layout.hit_test(Offset::new(500.0, 25.0)), 11);
        assert_eq!(layout.hit_test(Offset::new(-5.0, 100.0)), 12);
        assert_eq!(
            layout.hit_test_with_affinity(Offset::new(500.0, 5.0)),
            (6, Affinity::Upstream)
        );
    }

    #[test]
    fn moving_vertically_keeps_the_column() {
        let layout = sample();
        assert_eq!(
            layout.move_vertically(3, Affinity::Downstream, 1, None),
            Some((9, Affinity::Downstream))
        );
        assert_eq!(
            layout.move_vertically(3, Affinity::Downstream, -1, None),
            None
        );
        assert_eq!(
            layout.move_vertically(9, Affinity::Downstream, 1, None),
            Some((14, Affinity::Downstream))
        );
    }

    #[test]
    fn line_ends_are_visual() {
        let layout = sample();
        assert_eq!(
            layout.line_end(2, Affinity::Downstream),
            (6, Affinity::Upstream)
        );
        assert_eq!(
            layout.line_start(8, Affinity::Downstream),
            (6, Affinity::Downstream)
        );
        assert_eq!(
            layout.line_end(8, Affinity::Downstream),
            (11, Affinity::Downstream)
        );
    }

    #[test]
    fn selection_spans_lines_and_marks_breaks() {
        let layout = sample();
        let rects = layout.selection_rects(4, 14);
        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0].origin.x, 40.0);
        assert_eq!(rects[0].size.width, 20.0);
        // The hard break after "world" is included with extra width.
        assert!(rects[1].size.width > 50.0);
        assert_eq!(rects[2].size.width, 20.0);
    }

    #[test]
    fn char_positions_cover_every_boundary() {
        let layout = sample();
        let positions = layout.char_positions();
        // 14 characters and the end.
        assert_eq!(positions.len(), 15);
        assert_eq!(positions[6], (0.0, 1));
        assert_eq!(positions[11], (50.0, 1));
        assert_eq!(positions[12], (0.0, 2));
        assert_eq!(positions[14], (20.0, 2));
    }

    #[test]
    fn right_to_left_clusters_flip_the_caret() {
        let rtl = |start: usize, x: f32| Cluster {
            start,
            end: start + 1,
            x,
            width: 10.0,
            rtl: true,
            span: 0,
        };
        let layout = TextLayout::new(
            "abc",
            Vec::new(),
            14.0,
            vec![LineLayout {
                start: 0,
                end: 3,
                top: 0.0,
                height: 20.0,
                baseline: 15.0,
                width: 30.0,
                rtl: true,
                hard_break: true,
                clusters: vec![rtl(2, 0.0), rtl(1, 10.0), rtl(0, 20.0)],
            }],
        );
        assert_eq!(layout.caret(0, Affinity::Downstream).x, 30.0);
        assert_eq!(layout.caret(3, Affinity::Downstream).x, 0.0);
        assert_eq!(layout.hit_test(Offset::new(28.0, 5.0)), 0);
        assert_eq!(layout.hit_test(Offset::new(1.0, 5.0)), 3);
    }
}
