//! Selecting across several pieces of text as if they were one document, as Compose's
//! `SelectionContainer` and `DisableSelection` do.
//!
//! Every text composed inside a [`selection_container`] takes part: [`text`](crate::text)
//! and [`text_styled`](crate::text_styled), [`text_ellipsized`](crate::text_ellipsized)
//! (its visible part), [`rich_text`](crate::rich_text) (so also
//! [`marked_text`](crate::marked_text)) and [`selectable_text`](crate::selectable_text),
//! wherever they sit below the container, lazy list items included while they are
//! composed. [`disable_selection`] leaves a part out. Each text registers itself where it
//! is placed, each time the display is built, so text whose node is kept and emitted again
//! without being composed takes part too, and the document order is the order the texts
//! are placed (painted) in.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect};
use rustle_graphics::Color;
use rustle_runtime::{CompositionLocal, State, group, is_composing, mutable_state, remember};
use rustle_text::TextLayout;
use rustle_ui::{Focus, Key, KeyEvent, Modifier, clipboard, remember_focus};

use crate::linear::column_with;

/// The colors of selected text, as Compose's `TextSelectionColors`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextSelectionColors {
    /// The color of the handles that drag a selection on a touch screen.
    pub handle_color: Color,
    /// The color selected text is highlighted in.
    pub background_color: Color,
}

impl Default for TextSelectionColors {
    /// A blue highlight and handles of the same blue.
    fn default() -> Self {
        Self {
            handle_color: Color::rgb(74, 96, 214),
            background_color: Color::rgba(74, 96, 214, 140),
        }
    }
}

/// The colors of selected text in effect, Compose's `LocalTextSelectionColors`. Provide
/// another value to recolor the selection of every text field and selectable text below.
pub static LOCAL_TEXT_SELECTION_COLORS: CompositionLocal<TextSelectionColors> =
    CompositionLocal::new(TextSelectionColors::default);

/// A place in a selectable document: a block, in document order, and a character offset
/// within its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SelectionPoint {
    /// Which block, counting the texts inside the container in document order from 0: by
    /// where they were placed in the latest frame, top to bottom, then left to right, as
    /// Compose sorts the selectables of a `SelectionContainer`.
    pub block: usize,
    /// Characters from the start of the block's text.
    pub offset: usize,
}

/// One text inside a container as it was placed in the latest frame.
#[derive(Clone)]
struct Block {
    /// Which text it is, for as long as its node lives, as Compose's `selectableId`.
    key: u64,
    /// What it says: for text cut short with an ellipsis, the part that shows.
    text: Rc<str>,
    /// Where it is in the window.
    rect: Rect,
    /// Its shaped text, once laid out.
    layout: Option<Rc<TextLayout>>,
}

/// How far a press and the drag after it reach, as Compose's `SelectionAdjustment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    /// Character by character: a single press.
    Character,
    /// Word by word: a double press.
    Word,
    /// Paragraph by paragraph, a paragraph being a run of a text between line breaks: a
    /// triple press.
    Paragraph,
}

/// The press that is selecting, while the button is held.
#[derive(Debug, Clone, Copy)]
struct Gesture {
    /// How far it reaches.
    unit: Unit,
    /// The start of what the press took.
    start: SelectionPoint,
    /// The end of what the press took.
    end: SelectionPoint,
}

/// One end of a selection: the text it is in, by key, and the place in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct End {
    /// The text it is in.
    key: u64,
    /// Where that text stood in document order when the end was set, used while it is not
    /// placed.
    block: usize,
    /// Characters from the start of that text.
    offset: usize,
}

/// A selection: where it began, where it ends now, and the characters of each text it
/// covers, as Compose's selection and its `subselections`, worked out when it is set.
#[derive(Debug, Clone, PartialEq)]
struct Selected {
    /// Where it began.
    anchor: End,
    /// Where it ends now.
    focus: End,
    /// The characters of each text it covers, by key.
    parts: Rc<HashMap<u64, (usize, usize)>>,
}

/// What a container shares with the texts inside it.
#[derive(Default)]
struct Inner {
    /// The texts placed in the latest frame, in the order they were placed.
    blocks: RefCell<Vec<Block>>,
    /// Where the container is in the window, set each time it is placed.
    origin: Cell<Offset>,
    /// The press selecting, while the button is held.
    gesture: Cell<Option<Gesture>>,
}

/// What a [`selection_container_with`] holds: the selection and the text it spans. Two
/// handles are equal when they are the same holder.
#[derive(Clone)]
pub struct SelectionState {
    /// The texts and the gesture, shared with the texts inside the container.
    inner: Rc<Inner>,
    /// The selection.
    selection: State<Option<Selected>>,
}

impl PartialEq for SelectionState {
    /// Whether both handles are the same holder.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Default for SelectionState {
    /// A holder with nothing selected, as [`SelectionState::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SelectionState {
    /// How many texts were placed and what is selected.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SelectionState")
            .field("blocks", &self.inner.blocks.borrow().len())
            .field("selection", &self.selection())
            .finish()
    }
}

/// The container the text being composed is in, if any: its selection, and the key the next
/// text composed in it for the first time gets, kept by the container's composition.
#[derive(Clone)]
pub(crate) struct Scope {
    /// The container's selection.
    state: SelectionState,
    /// The key the next text gets.
    keys: Rc<Cell<u64>>,
}

impl PartialEq for Scope {
    /// Whether both are the same container's.
    fn eq(&self, other: &Self) -> bool {
        self.state == other.state && Rc::ptr_eq(&self.keys, &other.keys)
    }
}

/// The container the text being composed is in, if any.
pub(crate) static LOCAL_SELECTION: CompositionLocal<Option<Scope>> = CompositionLocal::new(|| None);

/// The first of the keys given to texts placed in a container without being composed, which
/// no text composed in one gets.
const LOOSE_KEYS: u64 = 1 << 63;

thread_local! {
    /// The key the next text placed without being composed gets.
    static NEXT_LOOSE_KEY: Cell<u64> = const { Cell::new(LOOSE_KEYS) };
}

/// The key of the text being composed in the container `keys` counts for, as Compose's
/// `selectableId`: remembered where the text is composed, so it keeps it while it stays in
/// the composition, and handed out in the order texts are first composed, so a composition
/// of the same content made again gives the same keys.
fn block_key(keys: &Rc<Cell<u64>>) -> u64 {
    if !is_composing() {
        return NEXT_LOOSE_KEY.with(|next| {
            let key = next.get();
            next.set(key + 1);
            key
        });
    }
    let keys = Rc::clone(keys);
    remember(move || {
        let key = keys.get();
        keys.set(key + 1);
        key
    })
    .get()
}

/// Distance along one axis from `value` to the span from `low` to `high`.
fn gap(value: f32, low: f32, high: f32) -> f32 {
    (low - value).max(value - high).max(0.0)
}

/// The characters from `start` to `end` of `text`.
fn slice(text: &str, start: usize, end: usize) -> String {
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

/// The byte offset of character `index` of `text`, or its length past the end.
fn byte_of(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// The start and end of the word around character `index`: letters and digits it
/// touches, or the single character otherwise.
fn word_bounds(text: &str, index: usize) -> (usize, usize) {
    let characters: Vec<char> = text.chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let index = index.min(characters.len());
    let mut start = index;
    while start > 0 && word(characters[start - 1]) {
        start -= 1;
    }
    let mut end = index;
    while end < characters.len() && word(characters[end]) {
        end += 1;
    }
    if start == end && end < characters.len() {
        end += 1;
    }
    (start, end)
}

/// The start and end of the paragraph around character `index`: the run of `text` between
/// the line breaks before and after it, as Compose's `SelectionAdjustment.Paragraph` takes.
fn paragraph_bounds(text: &str, index: usize) -> (usize, usize) {
    let characters: Vec<char> = text.chars().collect();
    let index = index.min(characters.len());
    let start = characters[..index]
        .iter()
        .rposition(|c| *c == '\n')
        .map_or(0, |at| at + 1);
    let end = characters[index..]
        .iter()
        .position(|c| *c == '\n')
        .map_or(characters.len(), |at| index + at);
    (start, end)
}

/// `blocks` in document order: top to bottom by the top of each, then left to right.
fn sorted(blocks: &[Block]) -> Vec<Block> {
    let mut blocks = blocks.to_vec();
    blocks.sort_by(|a, b| {
        a.rect
            .origin
            .y
            .total_cmp(&b.rect.origin.y)
            .then(a.rect.origin.x.total_cmp(&b.rect.origin.x))
    });
    blocks
}

/// The characters of block `index`, whose text is `text`, that a selection from `start` to
/// `end` covers, if any.
fn covered(
    start: SelectionPoint,
    end: SelectionPoint,
    index: usize,
    text: &str,
) -> Option<(usize, usize)> {
    if index < start.block || index > end.block {
        return None;
    }
    let length = text.chars().count();
    let from = if index == start.block {
        start.offset
    } else {
        0
    };
    let to = if index == end.block {
        end.offset
    } else {
        length
    };
    let (from, to) = (from.min(length), to.min(length));
    (from < to).then_some((from, to))
}

impl SelectionState {
    /// A holder with nothing selected, made outside composition, for a state holder that
    /// owns the selection of a container, as `ScrollState::new` is for scrolling. Reading
    /// its selection while composing makes that composition compose again when the
    /// selection changes.
    pub fn new() -> Self {
        Self {
            inner: Rc::new(Inner::default()),
            selection: mutable_state(None),
        }
    }

    /// The texts placed in the latest frame, in document order.
    fn document(&self) -> Vec<Block> {
        sorted(&self.inner.blocks.borrow())
    }

    /// Where `end` stands in `document`: the place of its text, or where that text stood
    /// when it is no longer placed.
    fn resolve(document: &[Block], end: End) -> SelectionPoint {
        let block = document
            .iter()
            .position(|block| block.key == end.key)
            .unwrap_or_else(|| end.block.min(document.len().saturating_sub(1)));
        SelectionPoint {
            block,
            offset: end.offset,
        }
    }

    /// The selection as its two ends in document order, or `None` if nothing is selected.
    pub fn selection(&self) -> Option<(SelectionPoint, SelectionPoint)> {
        let selected = self.selection.get()?;
        if selected.parts.is_empty() {
            return None;
        }
        let document = self.document();
        let (a, b) = (
            Self::resolve(&document, selected.anchor),
            Self::resolve(&document, selected.focus),
        );
        Some((a.min(b), a.max(b)))
    }

    /// Selects from `anchor` to `focus`, which may be in any order, among the texts placed
    /// in the latest frame.
    pub fn select(&self, anchor: SelectionPoint, focus: SelectionPoint) {
        let document = self.document();
        let end = |point: SelectionPoint| {
            document.get(point.block).map(|block| End {
                key: block.key,
                block: point.block,
                offset: point.offset,
            })
        };
        let (Some(anchor_end), Some(focus_end)) = (end(anchor), end(focus)) else {
            self.clear();
            return;
        };
        let (start, last) = (anchor.min(focus), anchor.max(focus));
        let parts: HashMap<u64, (usize, usize)> = document
            .iter()
            .enumerate()
            .filter_map(|(index, block)| {
                Some((block.key, covered(start, last, index, &block.text)?))
            })
            .collect();
        let selected = Selected {
            anchor: anchor_end,
            focus: focus_end,
            parts: Rc::new(parts),
        };
        if self.selection.get().as_ref() != Some(&selected) {
            self.selection.set(Some(selected));
        }
    }

    /// Selects all the text in the container, as it was placed in the latest frame.
    pub fn select_all(&self) {
        let blocks = self.inner.blocks.borrow();
        if blocks.is_empty() {
            return;
        }
        let document = sorted(&blocks);
        drop(blocks);
        let last = document.len() - 1;
        self.select(
            SelectionPoint {
                block: 0,
                offset: 0,
            },
            SelectionPoint {
                block: last,
                offset: document[last].text.chars().count(),
            },
        );
    }

    /// Removes the selection.
    pub fn clear(&self) {
        if self.selection.get().is_some() {
            self.selection.set(None);
        }
        self.inner.gesture.set(None);
    }

    /// The selected text, the part of each text joined by line breaks, as Compose's
    /// `SelectionContainer` copies it; `None` when nothing is selected.
    pub fn selected_text(&self) -> Option<String> {
        let selected = self.selection.get()?;
        let parts: Vec<String> = self
            .document()
            .iter()
            .filter_map(|block| {
                let (from, to) = selected.parts.get(&block.key)?;
                Some(slice(&block.text, *from, *to))
            })
            .collect();
        (!parts.is_empty()).then(|| parts.join("\n"))
    }

    /// The texts placed in the latest frame, in document order, each whole.
    pub fn texts(&self) -> Vec<String> {
        self.document()
            .iter()
            .map(|block| block.text.to_string())
            .collect()
    }

    /// Records the text `key`, saying `text`, placed at `rect` in the window.
    fn register(&self, key: u64, text: Rc<str>, rect: Rect, layout: Option<Rc<TextLayout>>) {
        self.inner.blocks.borrow_mut().push(Block {
            key,
            text,
            rect,
            layout,
        });
    }

    /// The characters of the text `key` the selection covers.
    fn covering(&self, key: u64) -> Option<(usize, usize)> {
        self.selection.get()?.parts.get(&key).copied()
    }

    /// The place in the document nearest to `point`, in the window: in the text under it,
    /// else the nearest text on the nearest row, its start when the point is above it and
    /// its end when below.
    fn point_at(&self, point: Offset) -> Option<SelectionPoint> {
        let document = self.document();
        let distance = |block: &Block| {
            let rect = block.rect;
            (
                gap(point.y, rect.origin.y, rect.origin.y + rect.size.height),
                gap(point.x, rect.origin.x, rect.origin.x + rect.size.width),
            )
        };
        let (index, block) = document.iter().enumerate().min_by(|(_, a), (_, b)| {
            let (a, b) = (distance(a), distance(b));
            a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1))
        })?;
        let rect = block.rect;
        let length = block.text.chars().count();
        let offset = if point.y < rect.origin.y {
            0
        } else if point.y > rect.origin.y + rect.size.height {
            length
        } else {
            block.layout.as_ref().map_or(0, |layout| {
                let local = Offset::new(point.x - rect.origin.x, point.y - rect.origin.y);
                let byte = layout.hit_test(local);
                layout.text().get(..byte).map_or(0, |s| s.chars().count())
            })
        };
        Some(SelectionPoint {
            block: index,
            offset: offset.min(length),
        })
    }

    /// The stretch around `point` that `unit` takes, within its block.
    fn unit_around(&self, point: SelectionPoint, unit: Unit) -> (SelectionPoint, SelectionPoint) {
        let text = self
            .document()
            .get(point.block)
            .map(|block| Rc::clone(&block.text))
            .unwrap_or_else(|| Rc::from(""));
        let (start, end) = match unit {
            Unit::Character => (point.offset, point.offset),
            Unit::Word => word_bounds(&text, point.offset),
            Unit::Paragraph => paragraph_bounds(&text, point.offset),
        };
        let at = |offset| SelectionPoint {
            block: point.block,
            offset,
        };
        (at(start), at(end))
    }

    /// Takes a press at `at`, in the window: a selection begins there, or from the place
    /// the selection there was began when `extend` asks for that, reaching as far as
    /// `clicks` presses in a row take.
    fn press(&self, at: Offset, clicks: u32, extend: bool) {
        let Some(point) = self.point_at(at) else {
            self.clear();
            return;
        };
        if extend
            && clicks == 1
            && let Some(selected) = self.selection.get()
        {
            let anchor = Self::resolve(&self.document(), selected.anchor);
            self.inner.gesture.set(Some(Gesture {
                unit: Unit::Character,
                start: anchor,
                end: anchor,
            }));
            self.select(anchor, point);
            return;
        }
        let unit = match clicks {
            2 => Unit::Word,
            3 => Unit::Paragraph,
            _ => Unit::Character,
        };
        let (start, end) = self.unit_around(point, unit);
        self.inner.gesture.set(Some(Gesture { unit, start, end }));
        self.select(start, end);
    }

    /// Extends the selection of the press held to `at`, in the window, a unit at a time as
    /// the press took them: Compose's selection adjustment while dragging.
    fn drag(&self, at: Offset) {
        let (Some(gesture), Some(point)) = (self.inner.gesture.get(), self.point_at(at)) else {
            return;
        };
        if gesture.unit == Unit::Character {
            self.select(gesture.start, point);
            return;
        }
        let (start, end) = self.unit_around(point, gesture.unit);
        if point < gesture.start {
            self.select(gesture.end, start);
        } else {
            self.select(gesture.start, end.max(gesture.end));
        }
    }

    /// Ends the press held, letting go of a selection that holds nothing.
    fn release(&self) {
        self.inner.gesture.set(None);
        if self
            .selection
            .get()
            .is_some_and(|selected| selected.parts.is_empty())
        {
            self.selection.set(None);
        }
    }

    /// Selects all with Ctrl or Cmd and `A`, and copies with Ctrl or Cmd and `C`.
    fn handle_key(&self, event: &KeyEvent) -> bool {
        if !event.pressed || !event.modifiers.command() {
            return false;
        }
        let Key::Character(letter) = &event.key else {
            return false;
        };
        match letter.to_lowercase().as_str() {
            "a" => {
                self.select_all();
                true
            }
            "c" => {
                if let (Some(text), Some(clipboard)) = (self.selected_text(), clipboard()) {
                    clipboard.set_text(&text);
                }
                true
            }
            _ => false,
        }
    }
}

/// Remembers the selection of a container at this call site.
#[track_caller]
pub fn remember_selection_state() -> SelectionState {
    group(|| remember(SelectionState::new).get())
}

/// Emits a column in which dragging selects across every text inside it, as if they were
/// one document: Compose's `SelectionContainer`.
///
/// A press starts a selection at the place nearest to it (Shift-press extends the selection
/// there was), a double press takes a word and a triple press a paragraph, and dragging
/// after either extends by words or paragraphs, across texts. Ctrl or Cmd with `C` copies
/// (texts joined by line breaks) and with `A` selects all. The highlight is drawn in the
/// [`LOCAL_TEXT_SELECTION_COLORS`] in effect where each text is composed. Arrange the content
/// with `column`, `row` and the rest inside; texts do not need to be direct children.
#[track_caller]
pub fn selection_container(modifier: Modifier, content: impl FnOnce()) {
    group(|| {
        let state = remember_selection_state();
        selection_container_with(modifier, &state, content);
    });
}

/// Like [`selection_container`], with the selection in `state`, so the surroundings
/// can read the selected text, select all or clear it.
#[track_caller]
pub fn selection_container_with(
    modifier: Modifier,
    state: &SelectionState,
    content: impl FnOnce(),
) {
    group(|| {
        state.selection.get();
        let focus: Focus = remember_focus();
        let (placed, key_state, press_state, drag_state, end_state) = (
            state.clone(),
            state.clone(),
            state.clone(),
            state.clone(),
            state.clone(),
        );
        let modifier = modifier
            .on_placed(move |rect, _| {
                placed.inner.blocks.borrow_mut().clear();
                placed.inner.origin.set(rect.origin);
            })
            .focusable(&focus, move |event| key_state.handle_key(event))
            .on_press(move |press| {
                let origin = press_state.inner.origin.get();
                let at = Offset::new(origin.x + press.position.x, origin.y + press.position.y);
                press_state.press(at, press.click_count, press.modifiers.shift);
            })
            .on_drag_end(move || end_state.release())
            .on_drag(move |position| {
                let origin = drag_state.inner.origin.get();
                drag_state.drag(Offset::new(origin.x + position.x, origin.y + position.y));
            });
        let keys = remember(|| Rc::new(Cell::new(0_u64))).get();
        let scope = Scope {
            state: state.clone(),
            keys,
        };
        LOCAL_SELECTION.provide(Some(scope), || column_with(modifier, content));
    });
}

/// Composes `content` with the texts in it left out of the selection container around it,
/// as Compose's `DisableSelection`.
#[track_caller]
pub fn disable_selection(content: impl FnOnce()) {
    LOCAL_SELECTION.provide(None, content);
}

/// Whether the text being composed is inside a selection container.
pub(crate) fn in_selection_container() -> bool {
    LOCAL_SELECTION.current().is_some()
}

/// What a text says inside a container, with its shaped text.
pub(crate) type ShapedText = (Rc<str>, Rc<TextLayout>);

/// What a text gives the container it takes part in when it is placed at a rectangle in
/// the window: what it says and its shaped text.
pub(crate) type Shaped = Rc<dyn Fn(Rect) -> Option<ShapedText>>;

/// `modifier` making the text it holds, `text`, take part in the selection container it is
/// composed in, if any: its shaped text is kept as layout measures it, it registers where it
/// is placed, and the part of it the selection covers is highlighted behind it.
pub(crate) fn selectable_modifier(modifier: Modifier, text: impl Into<Rc<str>>) -> Modifier {
    if LOCAL_SELECTION.current().is_none() {
        return modifier;
    }
    let text: Rc<str> = text.into();
    let shaped: Rc<RefCell<Option<Rc<TextLayout>>>> = Rc::default();
    let laid = Rc::clone(&shaped);
    let modifier = modifier.on_text_layout(move |layout| *laid.borrow_mut() = Some(layout));
    taking_part(
        modifier,
        Rc::new(move |_| Some((Rc::clone(&text), shaped.borrow().clone()?))),
    )
}

/// `modifier` registering the text `shaped` gives, when placed, in the selection container
/// it is composed in, if any, and highlighting behind it the part the selection covers.
pub(crate) fn taking_part(modifier: Modifier, shaped: Shaped) -> Modifier {
    let Some(Scope { state, keys }) = LOCAL_SELECTION.current() else {
        return modifier;
    };
    let key = block_key(&keys);
    let color = LOCAL_TEXT_SELECTION_COLORS.current().background_color;
    let painted: Rc<RefCell<Option<ShapedText>>> = Rc::default();
    let (held, holder) = (Rc::clone(&painted), state.clone());
    modifier
        .on_placed(move |rect, _| {
            let found = shaped(rect);
            let text = found
                .as_ref()
                .map_or_else(|| Rc::from(""), |(text, _)| Rc::clone(text));
            let layout = found.as_ref().map(|(_, layout)| Rc::clone(layout));
            holder.register(key, text, rect, layout);
            *held.borrow_mut() = found;
        })
        .draw_behind(move |canvas| {
            let Some((_, layout)) = painted.borrow().clone() else {
                return;
            };
            if let Some((from, to)) = state.covering(key) {
                let (start, end) = (byte_of(layout.text(), from), byte_of(layout.text(), to));
                canvas.selection(0.0, 0.0, &layout, start, end, color);
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_runs_of_letters_and_digits() {
        assert_eq!(word_bounds("hello big_world 42", 7), (6, 15));
        assert_eq!(word_bounds("hello big_world 42", 5), (0, 5));
        assert_eq!(word_bounds("a, b", 1), (0, 1));
        assert_eq!(word_bounds("a, b", 2), (2, 3));
        assert_eq!(word_bounds("", 0), (0, 0));
    }

    #[test]
    fn paragraphs_run_between_line_breaks() {
        assert_eq!(paragraph_bounds("one\ntwo words\nthree", 6), (4, 13));
        assert_eq!(paragraph_bounds("one\ntwo", 1), (0, 3));
        assert_eq!(paragraph_bounds("one\ntwo", 7), (4, 7));
        assert_eq!(paragraph_bounds("", 0), (0, 0));
    }

    #[test]
    fn slices_count_characters() {
        assert_eq!(slice("h\u{e9}llo w\u{f6}rld", 1, 8), "\u{e9}llo w\u{f6}");
        assert_eq!(slice("abc", 2, 99), "c");
        assert_eq!(slice("abc", 2, 1), "");
        assert_eq!(byte_of("h\u{e9}llo", 2), 3);
        assert_eq!(byte_of("ab", 9), 2);
    }

    #[test]
    fn gaps_are_zero_inside_a_span() {
        assert_eq!(gap(5.0, 0.0, 10.0), 0.0);
        assert_eq!(gap(-3.0, 0.0, 10.0), 3.0);
        assert_eq!(gap(14.0, 0.0, 10.0), 4.0);
    }

    #[test]
    fn points_order_by_block_then_offset() {
        let a = SelectionPoint {
            block: 0,
            offset: 9,
        };
        let b = SelectionPoint {
            block: 1,
            offset: 0,
        };
        assert!(a < b);
    }

    #[test]
    fn a_selection_covers_the_blocks_between_its_ends() {
        let start = SelectionPoint {
            block: 1,
            offset: 2,
        };
        let end = SelectionPoint {
            block: 3,
            offset: 1,
        };
        assert_eq!(covered(start, end, 0, "abcd"), None);
        assert_eq!(covered(start, end, 1, "abcd"), Some((2, 4)));
        assert_eq!(covered(start, end, 2, "abcd"), Some((0, 4)));
        assert_eq!(covered(start, end, 3, "abcd"), Some((0, 1)));
        assert_eq!(covered(start, end, 4, "abcd"), None);
    }

    #[test]
    fn copying_joins_texts_with_line_breaks() {
        let state = SelectionState::new();
        for (key, text) in ["first text", "second", "third one"]
            .into_iter()
            .enumerate()
        {
            state.register(key as u64, Rc::from(text), Rect::default(), None);
        }
        state.select(
            SelectionPoint {
                block: 0,
                offset: 6,
            },
            SelectionPoint {
                block: 2,
                offset: 5,
            },
        );
        assert_eq!(
            state.selected_text().as_deref(),
            Some("text\nsecond\nthird")
        );
        state.select_all();
        assert_eq!(
            state.selected_text().as_deref(),
            Some("first text\nsecond\nthird one")
        );
        state.clear();
        assert_eq!(state.selected_text(), None);
    }

    #[test]
    fn document_order_runs_down_then_across_whatever_order_texts_are_placed_in() {
        let state = SelectionState::new();
        let at =
            |x: f32, y: f32| Rect::new(Offset::new(x, y), rustle_geometry::Size::new(10.0, 10.0));
        state.register(0, Rc::from("lowest"), at(0.0, 40.0), None);
        state.register(1, Rc::from("right"), at(50.0, 0.0), None);
        state.register(2, Rc::from("left"), at(0.0, 0.0), None);
        assert_eq!(state.texts(), vec!["left", "right", "lowest"]);
        state.select_all();
        assert_eq!(
            state.selected_text().as_deref(),
            Some("left\nright\nlowest")
        );
    }

    #[test]
    fn a_selection_keeps_to_its_texts_and_leaves_out_texts_placed_after_it_was_made() {
        let state = SelectionState::new();
        state.register(7, Rc::from("one"), Rect::default(), None);
        state.select_all();
        state.register(8, Rc::from("two"), Rect::default(), None);
        assert_eq!(state.selected_text().as_deref(), Some("one"));
        assert_eq!(state.covering(7), Some((0, 3)));
        assert_eq!(state.covering(8), None);
    }
}
