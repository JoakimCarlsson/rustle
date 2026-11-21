//! Annotated text as Compose's `BasicText(AnnotatedString, inlineContent)` sets it: spans in
//! their styles, paragraphs in their own styles, links that are pressed, hovered and focused,
//! and composables set in the lines in the room their placeholders keep.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Color, TextAlign};
use rustle_runtime::{key, remember};
use rustle_text::{
    Affinity, AnnotatedRange, AnnotatedString, AnnotatedStringBuilder, LinkAnnotation, Placeholder,
    PlaceholderVerticalAlign, SpanStyle, TextLayout, TextLinkStyles, TextSpan, TextStyle,
};
use rustle_ui::interaction_source::{InteractionId, MutableInteractionSource, PressInteraction};
use rustle_ui::pointer::{PointerEventPass, PointerEventType};
use rustle_ui::{
    Children, Content, CursorIcon, LOCAL_URI_HANDLER, Layout, LayoutResult, Modifier, Node,
    PointerButton, Role, Semantics, UriHandler, emit, layout,
};
use rustle_unit::Constraints;

use crate::selection::selectable_modifier;
use crate::text::LOCAL_TEXT_STYLE;

/// The tag of the string annotation [`AppendInlineContent::append_inline_content`] marks
/// inline content with, its annotation being the content's id.
pub const INLINE_CONTENT_TAG: &str = "rustle.foundation.text.inlineContent";

/// The alternate text inline content stands for when none is given, Compose's default.
pub const DEFAULT_ALTERNATE_TEXT: &str = "\u{FFFD}";

/// A composable set in a line of text, in the room its placeholder keeps: Compose's
/// `InlineTextContent`. `children` composes it, told the alternate text it stands for, and it
/// is measured at exactly the placeholder's size.
#[derive(Clone)]
pub struct InlineTextContent {
    /// The room it takes in the line.
    pub placeholder: Placeholder,
    /// Composes it, told the alternate text it stands for.
    pub children: Rc<dyn Fn(&str)>,
}

impl InlineTextContent {
    /// Content that takes the room of `placeholder` and is composed by `children`.
    pub fn new(placeholder: Placeholder, children: impl Fn(&str) + 'static) -> Self {
        Self {
            placeholder,
            children: Rc::new(children),
        }
    }
}

/// Inline content added to an annotated string, Compose's `appendInlineContent`.
pub trait AppendInlineContent {
    /// Appends `alternate_text`, marked as standing for the inline content `id`: the text is
    /// what selection and copying see, and the content is drawn in its place.
    ///
    /// # Panics
    ///
    /// When `alternate_text` is empty, as in Compose.
    fn append_inline_content(&mut self, id: &str, alternate_text: &str);
}

impl AppendInlineContent for AnnotatedStringBuilder {
    /// Pushes the inline content's annotation, appends the alternate text and pops it.
    fn append_inline_content(&mut self, id: &str, alternate_text: &str) {
        assert!(
            !alternate_text.is_empty(),
            "the alternate text of inline content is empty"
        );
        self.with_annotation(INLINE_CONTENT_TAG, id, |builder| {
            builder.append(alternate_text)
        });
    }
}

/// The character set where a placeholder keeps room, one per character of its alternate text
/// so the text laid out counts as many characters as the text selected.
const CARRIER: char = 'x';

/// One placeholder of a paragraph: where it is in the paragraph's text and how it is placed.
#[derive(Debug, Clone, Copy)]
struct Kept {
    /// The paragraph it is in.
    paragraph: usize,
    /// Its first byte in the paragraph's text.
    start: usize,
    /// The byte after its last in the paragraph's text.
    end: usize,
    /// Its size in logical pixels.
    size: Size,
    /// Where it sits on its line.
    align: PlaceholderVerticalAlign,
}

/// One link of a paragraph: where it is in the paragraph's text and how many areas take its
/// presses, one per line it can run over.
#[derive(Debug, Clone, Copy)]
struct Linked {
    /// The paragraph it is in.
    paragraph: usize,
    /// Its first byte in the paragraph's text.
    start: usize,
    /// The byte after its last in the paragraph's text.
    end: usize,
    /// How many areas it has.
    areas: usize,
}

/// The shaped text of a paragraph, as layout told it.
type LaidOut = Rc<RefCell<Option<Rc<TextLayout>>>>;

/// How annotated text lays its parts out: its paragraphs one under another, then each
/// placeholder's content at its room and each link's areas over its text.
struct AnnotatedLayout {
    /// The shaped text of each paragraph, in order.
    paragraphs: Vec<LaidOut>,
    /// How each paragraph's lines line up across its width.
    aligns: Vec<TextAlign>,
    /// The placeholders, in the order their contents are children after the paragraphs.
    placeholders: Vec<Kept>,
    /// The links, in the order their areas are children after the placeholders' contents.
    links: Vec<Linked>,
}

impl AnnotatedLayout {
    /// How far right `line` of a paragraph `width` wide starts, as `align` lines it up.
    fn line_shift(align: TextAlign, width: f32, line_width: f32) -> f32 {
        match align {
            TextAlign::Center => ((width - line_width) / 2.0).max(0.0),
            TextAlign::Right => (width - line_width).max(0.0),
            TextAlign::Start | TextAlign::Left => 0.0,
        }
    }

    /// The rectangles that cover the bytes from `start` to `end` of `layout`, each moved right
    /// as its paragraph's alignment moves its line, for a paragraph `width` wide.
    fn covered(
        layout: &TextLayout,
        align: TextAlign,
        width: f32,
        start: usize,
        end: usize,
    ) -> Vec<Rect> {
        layout
            .selection_rects(start, end)
            .into_iter()
            .map(|rect| {
                let line = layout.line_at_y(rect.origin.y + rect.size.height / 2.0);
                let line_width = layout.lines().get(line).map_or(0.0, |line| line.width);
                let shift = Self::line_shift(align, width, line_width);
                Rect::new(Offset::new(rect.origin.x + shift, rect.origin.y), rect.size)
            })
            .collect()
    }

    /// Where the top of `kept` goes in its paragraph's `layout`, as its alignment says.
    fn placeholder_top(layout: &TextLayout, kept: &Kept) -> f32 {
        let index = layout.line_of_offset(kept.start, Affinity::Downstream);
        let Some(line) = layout.lines().get(index) else {
            return 0.0;
        };
        let height = kept.size.height;
        let (text_top, text_bottom) = if kept.start > line.start {
            let caret = layout.caret(kept.start, Affinity::Upstream);
            (caret.y, caret.y + caret.height)
        } else if kept.end < line.end {
            let caret = layout.caret(kept.end, Affinity::Downstream);
            (caret.y, caret.y + caret.height)
        } else {
            (line.top, line.top + line.height)
        };
        match kept.align {
            PlaceholderVerticalAlign::AboveBaseline => line.top + line.baseline - height,
            PlaceholderVerticalAlign::Top => line.top,
            PlaceholderVerticalAlign::Bottom => line.top + line.height - height,
            PlaceholderVerticalAlign::Center => line.top + (line.height - height) / 2.0,
            PlaceholderVerticalAlign::TextTop => text_top,
            PlaceholderVerticalAlign::TextBottom => text_bottom - height,
            PlaceholderVerticalAlign::TextCenter => (text_top + text_bottom - height) / 2.0,
        }
    }

    /// The constraints that measure a child at exactly `size`.
    fn exactly(size: Size) -> Constraints {
        Constraints {
            min_width: size.width,
            max_width: size.width,
            min_height: size.height,
            max_height: size.height,
        }
    }

    /// The sum of what `query` answers for each paragraph.
    fn summed(&self, children: &mut Children, query: impl Fn(&mut Children, usize) -> f32) -> f32 {
        (0..self.paragraphs.len())
            .map(|index| query(children, index))
            .sum()
    }

    /// The largest of what `query` answers for each paragraph.
    fn widest(&self, children: &mut Children, query: impl Fn(&mut Children, usize) -> f32) -> f32 {
        (0..self.paragraphs.len())
            .map(|index| query(children, index))
            .fold(0.0, f32::max)
    }
}

impl Layout for AnnotatedLayout {
    /// Lays the paragraphs out one under another at the width offered, then measures each
    /// placeholder's content at its size and puts it in its room, and puts each link's areas
    /// over the lines its text runs over; areas a link does not need are left empty.
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let loose = Constraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: constraints.max_width,
            max_height: f32::INFINITY,
        };
        let mut positions = vec![Offset::default(); children.len()];
        let mut tops = Vec::with_capacity(self.paragraphs.len());
        let mut widths = Vec::with_capacity(self.paragraphs.len());
        let mut height = 0.0_f32;
        for (index, position) in positions.iter_mut().enumerate().take(self.paragraphs.len()) {
            let size = children.measure(index, loose);
            *position = Offset::new(0.0, height);
            tops.push(height);
            widths.push(size.width);
            height += size.height;
        }
        let width = widths.iter().copied().fold(0.0, f32::max);
        let size = constraints.constrain(Size::new(width, height));
        let laid = |paragraph: usize| self.paragraphs[paragraph].borrow().clone();
        let mut child = self.paragraphs.len();
        for kept in &self.placeholders {
            let place = laid(kept.paragraph).and_then(|layout| {
                let rects = Self::covered(
                    &layout,
                    self.aligns[kept.paragraph],
                    widths[kept.paragraph],
                    kept.start,
                    kept.end,
                );
                let left = rects.first()?.origin.x;
                Some(Offset::new(
                    left,
                    tops[kept.paragraph] + Self::placeholder_top(&layout, kept),
                ))
            });
            let measured = place.map_or(Size::default(), |_| kept.size);
            children.measure(child, Self::exactly(measured));
            positions[child] = place.unwrap_or_default();
            child += 1;
        }
        for link in &self.links {
            let rects = laid(link.paragraph).map_or_else(Vec::new, |layout| {
                Self::covered(
                    &layout,
                    self.aligns[link.paragraph],
                    widths[link.paragraph],
                    link.start,
                    link.end,
                )
            });
            for area in 0..link.areas {
                let rect = if area + 1 < link.areas {
                    rects.get(area).copied()
                } else {
                    rects
                        .get(area..)
                        .and_then(|rest| rest.iter().copied().reduce(joined))
                };
                let rect = rect.unwrap_or_default();
                children.measure(child, Self::exactly(rect.size));
                positions[child] = Offset::new(rect.origin.x, tops[link.paragraph] + rect.origin.y);
                child += 1;
            }
        }
        LayoutResult { size, positions }
    }

    /// The widest paragraph's least width.
    fn min_intrinsic_width(&self, children: &mut Children, height: f32) -> f32 {
        self.widest(children, |children, index| {
            children.min_intrinsic_width(index, height)
        })
    }

    /// The widest paragraph's width on as few lines as it has.
    fn max_intrinsic_width(&self, children: &mut Children, height: f32) -> f32 {
        self.widest(children, |children, index| {
            children.max_intrinsic_width(index, height)
        })
    }

    /// The paragraphs' heights at `width`, added up.
    fn min_intrinsic_height(&self, children: &mut Children, width: f32) -> f32 {
        self.summed(children, |children, index| {
            children.min_intrinsic_height(index, width)
        })
    }

    /// The paragraphs' heights at `width`, added up.
    fn max_intrinsic_height(&self, children: &mut Children, width: f32) -> f32 {
        self.summed(children, |children, index| {
            children.max_intrinsic_height(index, width)
        })
    }
}

/// The smallest rectangle that holds both `a` and `b`.
fn joined(a: Rect, b: Rect) -> Rect {
    let left = a.origin.x.min(b.origin.x);
    let top = a.origin.y.min(b.origin.y);
    let right = (a.origin.x + a.size.width).max(b.origin.x + b.size.width);
    let bottom = (a.origin.y + a.size.height).max(b.origin.y + b.size.height);
    Rect::new(
        Offset::new(left, top),
        Size::new(right - left, bottom - top),
    )
}

/// How a link is being dealt with, read from its interaction source while composing.
struct LinkState {
    /// Where its presses, hovers and focus are reported.
    source: MutableInteractionSource,
    /// The style its text is drawn in now.
    style: SpanStyle,
}

/// The style a link with `styles` is drawn in as `source` says it is being dealt with. Only
/// the states a style is given for are read, so a link without a hovered style is not
/// composed again as the pointer comes over it.
fn link_style(styles: &TextLinkStyles, source: &MutableInteractionSource) -> SpanStyle {
    let read = |given: &Option<SpanStyle>, state: rustle_runtime::State<bool>| {
        given.is_some() && state.get()
    };
    styles.resolve(
        read(&styles.focused_style, source.collect_is_focused_as_state()),
        read(&styles.hovered_style, source.collect_is_hovered_as_state()),
        read(&styles.pressed_style, source.collect_is_pressed_as_state()),
    )
}

/// The run that keeps room for `placeholder` in text set in `style`, standing for
/// `alternate`: one carrier character per character of it, invisible, spaced so they add up
/// to the placeholder's width, and large enough that the line is at least as tall as the
/// placeholder.
fn carrier(placeholder: &Placeholder, alternate: &str, style: &TextStyle) -> (TextSpan, Size) {
    let size = Size::new(
        placeholder.width.to_px(style.font_size),
        placeholder.height.to_px(style.font_size),
    );
    let count = alternate.chars().count().max(1);
    let font_size = if size.height > style.line_height {
        size.height / style.line_height_ratio()
    } else {
        style.font_size
    };
    let plain = TextSpan::new(CARRIER.to_string())
        .font_size(font_size)
        .ligatures(false);
    let advance = rustle_text::layout_spans(std::slice::from_ref(&plain), style, None)
        .size()
        .width;
    let mut span = plain
        .color(Color::TRANSPARENT)
        .letter_spacing(size.width / count as f32 - advance);
    span.text = CARRIER.to_string().repeat(count);
    span.underline = Some(false);
    span.strikethrough = Some(false);
    (span, size)
}

/// The runs of the bytes from `start` to `end` of `text`, styled by its span styles and the
/// links' `links` styles over them, with the placeholders among them kept as carriers.
/// Returns the runs and the placeholders with the ids of their contents, their bytes counted
/// from `start` and their paragraph left at the first.
fn paragraph_runs(
    text: &AnnotatedString,
    (start, end): (usize, usize),
    links: &[AnnotatedRange<SpanStyle>],
    inline_content: &HashMap<String, InlineTextContent>,
    style: &TextStyle,
) -> (Vec<TextSpan>, Vec<(Kept, String)>) {
    let inline: Vec<AnnotatedRange<String>> = text
        .get_string_annotations(INLINE_CONTENT_TAG, start, end)
        .into_iter()
        .filter(|range| {
            range.start >= start
                && range.end <= end
                && range.start < range.end
                && inline_content.contains_key(&range.item)
        })
        .collect();
    let mut runs = Vec::new();
    let mut kept = Vec::new();
    let mut at = start;
    for range in &inline {
        if range.start < at {
            continue;
        }
        runs.extend(text.spans_in(at, range.start, links));
        let content = &inline_content[&range.item];
        let alternate = &text.text()[range.start..range.end];
        let (span, size) = carrier(&content.placeholder, alternate, style);
        runs.push(span);
        kept.push((
            Kept {
                paragraph: 0,
                start: range.start - start,
                end: range.end - start,
                size,
                align: content.placeholder.placeholder_vertical_align,
            },
            range.item.clone(),
        ));
        at = range.end;
    }
    runs.extend(text.spans_in(at, end, links));
    (runs, kept)
}

/// How many areas take the presses of a link saying `text`: one for each line it can run
/// over, which is one more than the places it can break.
fn link_areas(text: &str) -> usize {
    1 + text
        .char_indices()
        .filter(|(index, character)| {
            *index > 0 && (character.is_whitespace() || matches!(character, '-' | '/'))
        })
        .count()
}

/// What pressing `link` does: tells its listener, or opens its address through `handler`.
fn press_of(link: &LinkAnnotation, handler: UriHandler) -> impl Fn() + 'static {
    let link = link.clone();
    move || match (&link, link.listener()) {
        (_, Some(listener)) => listener(&link),
        (LinkAnnotation::Url { url, .. }, None) => handler.open_uri(url),
        (LinkAnnotation::Clickable { .. }, None) => {}
    }
}

/// The modifier of a link area that is not its first: a pointing hand that presses the
/// link and reports its hovers and presses to `source`, without being a focus target of its
/// own.
fn further_area(source: &MutableInteractionSource, on_click: impl Fn() + 'static) -> Modifier {
    let (pressed, released) = (source.clone(), source.clone());
    let held: Rc<Cell<Option<InteractionId>>> = Rc::default();
    let letting_go = Rc::clone(&held);
    Modifier::new()
        .cursor(CursorIcon::Pointer)
        .hoverable(source)
        .on_pointer_event(
            PointerEventType::Press,
            PointerEventPass::Main,
            move |event| {
                if event.button == Some(PointerButton::Primary)
                    && let Some(change) = event.changes.first()
                {
                    let id = InteractionId::next();
                    held.set(Some(id));
                    pressed.emit(PressInteraction::Press {
                        id,
                        press_position: change.position,
                    });
                }
            },
        )
        .on_pointer_event(
            PointerEventType::Release,
            PointerEventPass::Main,
            move |event| {
                if event.button == Some(PointerButton::Primary)
                    && let Some(press) = letting_go.take()
                {
                    released.emit(PressInteraction::Release { press });
                }
            },
        )
        .clickable_plain(on_click)
}

/// Emits `text` set in `style` with `modifier`, Compose's `BasicText(text, modifier, style,
/// inlineContent = inline_content)`.
///
/// Each span style is set over its range. Each range with a paragraph style is laid out as a
/// paragraph of its own under the text before it, with its own alignment and line height;
/// the paragraphs are as wide as the room offered and the text as wide as its widest. Each
/// link is drawn in its [`TextLinkStyles`](rustle_text::TextLinkStyles) as it is focused,
/// hovered and pressed, shows a pointing hand, is reached with Tab and, pressed with the
/// pointer or activated from the keyboard, tells its listener or, for a
/// [`LinkAnnotation::Url`] without one, opens its address through the
/// [`LOCAL_URI_HANDLER`](rustle_ui::LOCAL_URI_HANDLER). Each range marked by
/// [`AppendInlineContent::append_inline_content`] with an id `inline_content` holds keeps the
/// room of the content's placeholder in its line, the line growing to fit a taller one, and
/// the content is composed there; one whose id it does not hold is set as its alternate text.
///
/// Inside a [`selection_container`](crate::selection_container) each paragraph takes part in
/// the selection, inline content as the alternate text it stands for, and copying across
/// paragraphs puts a line break between them.
#[track_caller]
pub fn basic_text_annotated(
    modifier: Modifier,
    text: &AnnotatedString,
    style: TextStyle,
    inline_content: &HashMap<String, InlineTextContent>,
) {
    let links = text.get_link_annotations(0, text.len());
    let handler = LOCAL_URI_HANDLER.current();
    let states: Vec<LinkState> = links
        .iter()
        .enumerate()
        .map(|(index, link)| {
            key(index, || {
                let source = remember(MutableInteractionSource::new).get();
                let style = link
                    .item
                    .styles()
                    .map(|styles| link_style(styles, &source))
                    .unwrap_or_default();
                LinkState { source, style }
            })
        })
        .collect();
    let link_styles: Vec<AnnotatedRange<SpanStyle>> = links
        .iter()
        .zip(&states)
        .map(|(link, state)| AnnotatedRange::new(state.style.clone(), link.start, link.end))
        .collect();
    let paragraphs = text.paragraphs();
    let mut laid_out = Vec::new();
    let mut aligns = Vec::new();
    let mut placeholders = Vec::new();
    let mut contents = Vec::new();
    let mut emitted = Vec::new();
    for (index, paragraph) in paragraphs.iter().enumerate() {
        let paragraph_style = paragraph.item.apply(style);
        let (runs, kept) = paragraph_runs(
            text,
            (paragraph.start, paragraph.end),
            &link_styles,
            inline_content,
            &paragraph_style,
        );
        for (found, id) in kept {
            let (start, end) = (paragraph.start + found.start, paragraph.start + found.end);
            placeholders.push(Kept {
                paragraph: index,
                ..found
            });
            contents.push((id, text.text()[start..end].to_owned()));
        }
        aligns.push(paragraph_style.align);
        let cell: LaidOut = Rc::default();
        laid_out.push(Rc::clone(&cell));
        let said = text.text()[paragraph.start..paragraph.end].to_owned();
        let carries = placeholders
            .iter()
            .any(|kept: &Kept| kept.paragraph == index);
        emitted.push((runs, paragraph_style, said, cell, carries));
    }
    let linked: Vec<Linked> = links
        .iter()
        .filter_map(|link| {
            let (index, paragraph) = paragraphs.iter().enumerate().find(|(_, paragraph)| {
                paragraph.start <= link.start && link.start < paragraph.end
            })?;
            let end = link.end.min(paragraph.end);
            Some(Linked {
                paragraph: index,
                start: link.start - paragraph.start,
                end: end - paragraph.start,
                areas: link_areas(&text.text()[link.start..end]),
            })
        })
        .collect();
    let simple = placeholders.is_empty() && links.is_empty() && emitted.len() == 1;
    if simple && let Some((runs, paragraph_style, said, _, _)) = emitted.pop() {
        emit(Node {
            modifier: selectable_modifier(modifier, said),
            content: Content::RichText {
                spans: runs,
                style: paragraph_style,
                editing: None,
            },
        });
        return;
    }
    let policy = AnnotatedLayout {
        paragraphs: laid_out,
        aligns,
        placeholders,
        links: linked.clone(),
    };
    layout(modifier, policy, move || {
        for (runs, paragraph_style, said, cell, carries) in emitted {
            let told =
                Modifier::new().on_text_layout(move |layout| *cell.borrow_mut() = Some(layout));
            let told = if carries {
                told.clear_and_set_semantics(Semantics::new(Role::Label).label(said.clone()))
            } else {
                told
            };
            emit(Node {
                modifier: selectable_modifier(told, said),
                content: Content::RichText {
                    spans: runs,
                    style: paragraph_style,
                    editing: None,
                },
            });
        }
        for (index, (id, alternate)) in contents.iter().enumerate() {
            let children = Rc::clone(&inline_content[id].children);
            key(("inline", index), || {
                layout(Modifier::new(), Stacked, || children(alternate));
            });
        }
        for ((link, state), place) in links.iter().zip(&states).zip(&linked) {
            for area in 0..place.areas {
                let on_click = press_of(&link.item, handler.clone());
                let modifier = if area == 0 {
                    Modifier::new().cursor(CursorIcon::Pointer).clickable_with(
                        &state.source,
                        None,
                        on_click,
                    )
                } else {
                    further_area(&state.source, on_click)
                };
                layout(modifier, Stacked, || {});
            }
        }
    });
}

/// Emits `text` with `modifier` in the current [`LOCAL_TEXT_STYLE`], with no inline content:
/// Compose's `BasicText(text: AnnotatedString)` with the style around it.
#[track_caller]
pub fn text_annotated(modifier: Modifier, text: &AnnotatedString) {
    basic_text_annotated(modifier, text, LOCAL_TEXT_STYLE.current(), &HashMap::new());
}

/// Puts every child at the top-left corner, at the size it is given: the box the content
/// of a placeholder or the area of a link is.
struct Stacked;

impl Layout for Stacked {
    /// Measures every child within `constraints` and takes the size they leave it.
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let mut size = Size::default();
        for index in 0..children.len() {
            let child = children.measure(index, constraints);
            size = Size::new(size.width.max(child.width), size.height.max(child.height));
        }
        LayoutResult {
            size: constraints.constrain(size),
            positions: vec![Offset::default(); children.len()],
        }
    }
}
