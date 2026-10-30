//! Selecting across several pieces of text as if they were one document.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustle_geometry::{Offset, Rect};
use rustle_graphics::Color;
use rustle_runtime::{CompositionLocal, State, group, remember};
use rustle_text::{Affinity, TextLayout};
use rustle_ui::{
    Content, Focus, Key, KeyEvent, Modifier, Node, TextEditing, clipboard, emit, remember_focus,
};

use crate::linear::column_with;

/// The color a selection is highlighted in.
pub(crate) const SELECTION_COLOR: Color = Color::rgba(74, 96, 214, 140);

/// A place in a selectable document: a block (in the order the blocks were composed)
/// and a character offset within its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SelectionPoint {
    /// Which block, counting `selectable_text` and `rich_text` calls inside the
    /// container in the order they were composed, from 0.
    pub block: usize,
    /// Characters from the start of the block's text.
    pub offset: usize,
}

type LayoutCell = Rc<RefCell<Option<Rc<TextLayout>>>>;

/// One piece of text inside a container, as of the latest composition.
struct Block {
    text: String,
    /// Where it is in the window, set after each layout.
    rect: Rc<Cell<Rect>>,
    layout: LayoutCell,
}

#[derive(Default)]
struct Inner {
    blocks: RefCell<Vec<Block>>,
    /// Where the container is in the window, set after each layout.
    origin: Cell<Offset>,
}

/// What a [`selection_container_with`] remembers: the selection and the text it spans.
#[derive(Clone)]
pub struct SelectionState {
    inner: Rc<Inner>,
    selection: State<Option<(SelectionPoint, SelectionPoint)>>,
}

impl PartialEq for SelectionState {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

/// The container the text being composed is in, if any.
pub(crate) static LOCAL_SELECTION: CompositionLocal<Option<SelectionState>> =
    CompositionLocal::new(|| None);

/// What a block needs to show and report its selection.
pub(crate) struct BlockHandle {
    pub(crate) rect: Rc<Cell<Rect>>,
    pub(crate) layout: LayoutCell,
    /// The selected characters of the block, as a start and end.
    pub(crate) selection: Option<(usize, usize)>,
}

impl BlockHandle {
    /// The editing details that make `Content::Text` or `Content::RichText` show the block's selection.
    pub(crate) fn editing(&self) -> TextEditing {
        TextEditing {
            caret: None,
            selection: self.selection,
            underline: None,
            ime_caret: None,
            caret_color: Color::TRANSPARENT,
            selection_color: SELECTION_COLOR,
            caret_affinity: Affinity::Downstream,
            boundaries: Rc::default(),
            layout: Rc::clone(&self.layout),
            on_layout: None,
        }
    }

    /// `modifier` that also reports where the block is.
    pub(crate) fn placed(&self, modifier: Modifier) -> Modifier {
        let rect = Rc::clone(&self.rect);
        modifier.on_placed(move |placed, _| rect.set(placed))
    }
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

impl SelectionState {
    /// The selection as its two ends in document order, or `None` if nothing is selected.
    pub fn selection(&self) -> Option<(SelectionPoint, SelectionPoint)> {
        let (a, b) = self.selection.get()?;
        (a != b).then(|| (a.min(b), a.max(b)))
    }

    /// Selects from `anchor` to `focus`, which may be in any order.
    pub fn select(&self, anchor: SelectionPoint, focus: SelectionPoint) {
        self.selection.set(Some((anchor, focus)));
    }

    /// Selects all the text in the container.
    pub fn select_all(&self) {
        let blocks = self.inner.blocks.borrow();
        let Some(last) = blocks.last() else {
            return;
        };
        let end = SelectionPoint {
            block: blocks.len() - 1,
            offset: last.text.chars().count(),
        };
        drop(blocks);
        self.select(
            SelectionPoint {
                block: 0,
                offset: 0,
            },
            end,
        );
    }

    /// Removes the selection.
    pub fn clear(&self) {
        if self.selection.get().is_some() {
            self.selection.set(None);
        }
    }

    /// The selected text, blocks separated by line breaks.
    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection()?;
        let blocks = self.inner.blocks.borrow();
        let mut parts = Vec::new();
        for (index, block) in blocks.iter().enumerate() {
            if index < start.block || index > end.block {
                continue;
            }
            let length = block.text.chars().count();
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
            parts.push(slice(&block.text, from, to.min(length)));
        }
        Some(parts.join("\n"))
    }

    /// Registers a block of `text` being composed and says what of it is selected.
    pub(crate) fn register(&self, text: String) -> BlockHandle {
        let length = text.chars().count();
        let mut blocks = self.inner.blocks.borrow_mut();
        let index = blocks.len();
        let selection = self.selection().and_then(|(start, end)| {
            if index < start.block || index > end.block {
                return None;
            }
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
            Some((from.min(length), to.min(length)))
        });
        let (rect, layout) = (Rc::new(Cell::new(Rect::default())), LayoutCell::default());
        blocks.push(Block {
            text,
            rect: Rc::clone(&rect),
            layout: Rc::clone(&layout),
        });
        BlockHandle {
            rect,
            layout,
            selection,
        }
    }

    /// The place in the document nearest to `point`, in the window.
    fn point_at(&self, point: Offset) -> Option<SelectionPoint> {
        let blocks = self.inner.blocks.borrow();
        let distance = |block: &Block| {
            let rect = block.rect.get();
            (
                gap(point.y, rect.origin.y, rect.origin.y + rect.size.height),
                gap(point.x, rect.origin.x, rect.origin.x + rect.size.width),
            )
        };
        let (index, block) = blocks.iter().enumerate().min_by(|(_, a), (_, b)| {
            let (a, b) = (distance(a), distance(b));
            a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1))
        })?;
        let rect = block.rect.get();
        let length = block.text.chars().count();
        let offset = if point.y < rect.origin.y {
            0
        } else if point.y > rect.origin.y + rect.size.height {
            length
        } else {
            let layout = block.layout.borrow().clone();
            layout.map_or(0, |layout| {
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

    fn block_text(&self, index: usize) -> Option<String> {
        self.inner
            .blocks
            .borrow()
            .get(index)
            .map(|b| b.text.clone())
    }

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
    group(|| SelectionState {
        inner: remember(|| Rc::new(Inner::default())).get(),
        selection: remember(|| None),
    })
}

/// Emits a column in which dragging selects across every [`selectable_text`](crate::selectable_text)
/// and [`rich_text`](crate::rich_text) inside it, as if they were one document.
/// Click and drag to select (Shift-click extends), double-click for a word, triple-click for a block, and
/// Ctrl or Cmd with `C` copies (blocks joined by line breaks) and with `A` selects all.
/// Blocks are numbered in the order they are composed. Arrange the content with
/// `column`, `row` and the rest inside; blocks do not need to be direct children.
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
        state.inner.blocks.borrow_mut().clear();
        let focus: Focus = remember_focus();
        let (key_state, press_state, drag_state, inner) = (
            state.clone(),
            state.clone(),
            state.clone(),
            Rc::clone(&state.inner),
        );
        let drag_anchor: Rc<Cell<Option<SelectionPoint>>> = Rc::default();
        let anchor_for_drag = Rc::clone(&drag_anchor);
        let modifier = modifier
            .on_placed(move |rect, _| inner.origin.set(rect.origin))
            .focusable(&focus, move |event| key_state.handle_key(event))
            .on_press(move |press| {
                let origin = press_state.inner.origin.get();
                let at = Offset::new(origin.x + press.position.x, origin.y + press.position.y);
                let Some(point) = press_state.point_at(at) else {
                    return;
                };
                // Shift extends the selection from where it began.
                if press.modifiers.shift
                    && press.click_count == 1
                    && let Some((anchor, _)) = press_state.selection.get()
                {
                    drag_anchor.set(Some(anchor));
                    press_state.select(anchor, point);
                    return;
                }
                drag_anchor.set(Some(point));
                match press.click_count {
                    2 => {
                        let text = press_state.block_text(point.block).unwrap_or_default();
                        let (start, end) = word_bounds(&text, point.offset);
                        let block = point.block;
                        drag_anchor.set(Some(SelectionPoint {
                            block,
                            offset: start,
                        }));
                        press_state.select(
                            SelectionPoint {
                                block,
                                offset: start,
                            },
                            SelectionPoint { block, offset: end },
                        );
                    }
                    3 => {
                        let length = press_state
                            .block_text(point.block)
                            .map_or(0, |text| text.chars().count());
                        let block = point.block;
                        drag_anchor.set(Some(SelectionPoint { block, offset: 0 }));
                        press_state.select(
                            SelectionPoint { block, offset: 0 },
                            SelectionPoint {
                                block,
                                offset: length,
                            },
                        );
                    }
                    _ => press_state.select(point, point),
                }
            })
            .on_drag(move |position| {
                let origin = drag_state.inner.origin.get();
                let at = Offset::new(origin.x + position.x, origin.y + position.y);
                let (Some(anchor), Some(point)) = (anchor_for_drag.get(), drag_state.point_at(at))
                else {
                    return;
                };
                if drag_state.selection.get() != Some((anchor, point)) {
                    drag_state.select(anchor, point);
                }
            });
        LOCAL_SELECTION.provide(Some(state.clone()), || column_with(modifier, content));
    });
}

/// Emits `content` as a selectable block of the container `state` belongs to.
pub(crate) fn emit_block(
    state: &SelectionState,
    modifier: Modifier,
    text: String,
    content: impl FnOnce(TextEditing) -> Content,
) {
    let block = state.register(text);
    let editing = block.editing();
    emit(Node {
        modifier: block.placed(modifier),
        content: content(editing),
    });
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
    fn slices_count_characters() {
        assert_eq!(slice("h\u{e9}llo w\u{f6}rld", 1, 8), "\u{e9}llo w\u{f6}");
        assert_eq!(slice("abc", 2, 99), "c");
        assert_eq!(slice("abc", 2, 1), "");
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
}
