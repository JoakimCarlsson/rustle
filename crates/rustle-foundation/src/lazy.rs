use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use rustle_runtime::{State, group, key, remember};
use rustle_ui::{Modifier, ScrollState, remember_scroll_state};

use crate::linear::{column_with, row_with, spacer, stack_with};

/// Emits a vertically scrolling list of `count` items, each `item_height` tall,
/// composing only the items near the viewport. `item` emits the item at an index.
///
/// Every item has the same height, so the list knows its length and what is
/// visible without composing the rest. Items keep their state as they scroll
/// out of the composed range and back, as long as they stay inside it; outside
/// it they leave the composition.
#[track_caller]
pub fn lazy_column(
    modifier: Modifier,
    state: &ScrollState,
    count: usize,
    item_height: f32,
    item: impl Fn(usize),
) {
    group(|| {
        let (first, last) = visible_range(state, count, item_height);
        column_with(modifier.vertical_scroll(state), || {
            spacer(Modifier::new().height(first as f32 * item_height));
            for index in first..last {
                key(index, || {
                    stack_with(Modifier::new().fill_max_width().height(item_height), || {
                        item(index)
                    });
                });
            }
            spacer(Modifier::new().height((count - last) as f32 * item_height));
        });
    });
}

/// Emits a vertically scrolling grid of `count` items in `columns` columns of
/// equal width, each row `row_height` tall, composing only the rows near the
/// viewport. `item` emits the item at an index.
#[track_caller]
pub fn lazy_vertical_grid(
    modifier: Modifier,
    state: &ScrollState,
    count: usize,
    columns: usize,
    row_height: f32,
    item: impl Fn(usize),
) {
    let columns = columns.max(1);
    let rows = count.div_ceil(columns);
    lazy_column(modifier, state, rows, row_height, |row| {
        row_with(Modifier::new().fill_max_size(), || {
            for column in 0..columns {
                let index = row * columns + column;
                key(column, || {
                    stack_with(Modifier::new().weight(1.0).fill_max_height(), || {
                        if index < count {
                            item(index);
                        }
                    });
                });
            }
        });
    });
}

/// Emits a horizontally scrolling list of `count` items, each `item_width` wide.
/// See [`lazy_column`].
#[track_caller]
pub fn lazy_row(
    modifier: Modifier,
    state: &ScrollState,
    count: usize,
    item_width: f32,
    item: impl Fn(usize),
) {
    group(|| {
        let (first, last) = visible_range(state, count, item_width);
        row_with(modifier.horizontal_scroll(state), || {
            spacer(Modifier::new().width(first as f32 * item_width));
            for index in first..last {
                key(index, || {
                    stack_with(Modifier::new().fill_max_height().width(item_width), || {
                        item(index)
                    });
                });
            }
            spacer(Modifier::new().width((count - last) as f32 * item_width));
        });
    });
}

/// The items of a fixed-size list worth composing: the visible ones and one more on each side.
fn visible_range(state: &ScrollState, count: usize, item_length: f32) -> (usize, usize) {
    // Until layout has measured the viewport, assume a generous one.
    let viewport = match state.viewport_length() {
        length if length > 0.0 => length,
        _ => 1200.0,
    };
    let first = (state.offset() / item_length).floor().max(0.0) as usize;
    let last = ((state.offset() + viewport) / item_length).ceil().max(0.0) as usize;
    (first.saturating_sub(1).min(count), (last + 1).min(count))
}

/// Where an item lands in the viewport when a list scrolls to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemAlign {
    /// The item's top edge at the top of the viewport.
    #[default]
    Start,
    /// The item centered in the viewport.
    Center,
    /// The item's bottom edge at the bottom of the viewport.
    End,
}

/// A scroll the list has been asked to make, done the next time it composes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Request {
    Item(usize, ItemAlign),
    End,
}

/// Where the items of a list were when it last composed.
#[derive(Default)]
struct Placed {
    /// The key of each item.
    keys: Vec<u64>,
    /// The top of each item and, last, the end of the list.
    tops: Vec<f32>,
    /// How tall the viewport was.
    viewport: f32,
}

struct ListInner {
    /// The measured height of each item, by key.
    heights: RefCell<HashMap<u64, f32>>,
    /// The sum of `heights`, for the average that estimates the rest.
    sum: Cell<f32>,
    placed: RefCell<Placed>,
    stick_to_end: Cell<bool>,
    request: Cell<Option<Request>>,
}

/// What a variable-height lazy list remembers: its scroll position and the
/// heights its items turned out to have.
#[derive(Clone)]
pub struct LazyListState {
    scroll: ScrollState,
    inner: Rc<ListInner>,
    /// Bumped when a height is learned, so the list composes again with it.
    version: State<u64>,
}

impl LazyListState {
    /// The scroll state, for reading the offset, the extent or scrolling.
    pub fn scroll(&self) -> &ScrollState {
        &self.scroll
    }

    /// Makes the list follow its end: while the user has not scrolled away
    /// from the end, items that arrive (or grow) keep it scrolled to the last
    /// one, as in a chat. Scrolling up unpins it; scrolling back to the end,
    /// or [`LazyListState::scroll_to_end`], pins it again. Off by default.
    pub fn set_stick_to_end(&self, stick: bool) {
        if self.inner.stick_to_end.replace(stick) != stick {
            self.bump();
        }
    }

    /// Whether the list follows its end. See [`LazyListState::set_stick_to_end`].
    pub fn stick_to_end(&self) -> bool {
        self.inner.stick_to_end.get()
    }

    /// Scrolls so that item `index` is placed in the viewport as `align` says.
    ///
    /// Items not measured yet count as the average height, so the list keeps
    /// the item where it was put as the ones around it are measured. Done
    /// the next time the list composes.
    pub fn scroll_to_item(&self, index: usize, align: ItemAlign) {
        self.inner.request.set(Some(Request::Item(index, align)));
        self.scroll.show_scrollbar();
        self.bump();
    }

    /// Scrolls to the end of the list, and pins it there if it follows its end.
    pub fn scroll_to_end(&self) {
        self.inner.request.set(Some(Request::End));
        self.scroll.show_scrollbar();
        self.bump();
    }

    /// The index of the first item in view as of the latest composition, if any.
    pub fn first_visible_item(&self) -> Option<usize> {
        let placed = self.inner.placed.borrow();
        let count = placed.keys.len();
        (count > 0).then(|| item_at(&placed.tops, self.scroll.offset()).min(count - 1))
    }

    fn bump(&self) {
        self.version.update(|version| *version += 1);
    }

    fn record(&self, key: u64, height: f32) {
        let changed = {
            let mut heights = self.inner.heights.borrow_mut();
            let old = heights.insert(key, height);
            self.inner
                .sum
                .set(self.inner.sum.get() + height - old.unwrap_or(0.0));
            old.is_none_or(|old| (old - height).abs() > 0.5)
        };
        if changed {
            self.bump();
        }
    }
}

/// Remembers a [`LazyListState`] at this call site.
#[track_caller]
pub fn remember_lazy_list_state() -> LazyListState {
    group(|| LazyListState {
        scroll: remember_scroll_state(),
        inner: remember(|| {
            Rc::new(ListInner {
                heights: RefCell::default(),
                sum: Cell::new(0.0),
                placed: RefCell::default(),
                stick_to_end: Cell::new(false),
                request: Cell::new(None),
            })
        })
        .get(),
        version: remember(|| 0),
    })
}

/// How close to the end counts as being at it, in pixels.
const END_TOLERANCE: f32 = 2.0;

/// The index of the item containing `offset`, given the item tops, which end
/// with the end of the list.
fn item_at(tops: &[f32], offset: f32) -> usize {
    tops.partition_point(|top| *top <= offset).saturating_sub(1)
}

/// The tops of items with the given `keys`, ending with the end of the list.
/// Items not measured count as `estimate`.
fn place(keys: &[u64], heights: &HashMap<u64, f32>, estimate: f32) -> Vec<f32> {
    let mut tops = Vec::with_capacity(keys.len() + 1);
    let mut y = 0.0;
    for key in keys {
        tops.push(y);
        y += heights.get(key).copied().unwrap_or(estimate);
    }
    tops.push(y);
    tops
}

/// What a list needs to decide where it is scrolled after its items changed.
struct Reposition<'a> {
    offset: f32,
    viewport: f32,
    /// How tall the viewport was when the list was last placed.
    old_viewport: f32,
    stick_to_end: bool,
    request: Option<Request>,
    old_keys: &'a [u64],
    old_tops: &'a [f32],
    new_keys: &'a [u64],
    new_tops: &'a [f32],
}

impl Reposition<'_> {
    /// The offset that keeps the list where the user left it.
    ///
    /// A request is done first. Otherwise, if the list follows its end and was
    /// scrolled to the end, it stays there. Otherwise the item that was at the
    /// top of the viewport stays exactly where it was, by key, however the
    /// items above it were measured, inserted or removed.
    fn offset(&self) -> f32 {
        let total = self.new_tops.last().copied().unwrap_or(0.0);
        let max = (total - self.viewport).max(0.0);
        match self.request {
            Some(Request::End) => return max,
            Some(Request::Item(index, align)) => {
                let Some(last) = self.new_keys.len().checked_sub(1) else {
                    return 0.0;
                };
                let index = index.min(last);
                let (top, bottom) = (self.new_tops[index], self.new_tops[index + 1]);
                let target = match align {
                    ItemAlign::Start => top,
                    ItemAlign::Center => top - (self.viewport - (bottom - top)) / 2.0,
                    ItemAlign::End => bottom - self.viewport,
                };
                return target.clamp(0.0, max);
            }
            None => {}
        }
        let Some(old_total) = self.old_tops.last().copied() else {
            // Nothing was laid out before: a list that follows its end starts there.
            return if self.stick_to_end { max } else { self.offset };
        };
        let old_end = (old_total - self.old_viewport).max(0.0);
        if self.stick_to_end && self.offset >= old_end - END_TOLERANCE {
            return max;
        }
        if self.old_keys.is_empty() {
            return self.offset;
        }
        let anchor = item_at(self.old_tops, self.offset).min(self.old_keys.len() - 1);
        let moved_by = self.offset - self.old_tops[anchor];
        let key = self.old_keys[anchor];
        let now = if self.new_keys.get(anchor) == Some(&key) {
            Some(anchor)
        } else {
            self.new_keys.iter().position(|candidate| *candidate == key)
        };
        match now {
            Some(index) => self.new_tops[index] + moved_by,
            None => self.offset,
        }
    }
}

fn hash_key(key: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// Emits a vertically scrolling list of `count` items of any heights,
/// composing only the items near the viewport. `item` emits the item at an index.
///
/// Heights are learned as items are laid out; until then an item counts as the
/// average of those known, so the length of the list is an estimate that
/// settles as it is scrolled. Items are identified by their index; use
/// [`lazy_list_keyed`] if items are inserted or removed above the ones in view.
///
/// While an item above the viewport is measured, the list scrolls by the
/// difference, so what is in view does not move. See
/// [`LazyListState::set_stick_to_end`] to follow the end of the list.
#[track_caller]
pub fn lazy_list(modifier: Modifier, state: &LazyListState, count: usize, item: impl Fn(usize)) {
    lazy_list_keyed(modifier, state, count, |index| index, item);
}

/// Like [`lazy_list`], with items identified by `key` rather than their
/// position. Heights and what an item remembers follow its key, so inserting
/// items above the ones in view (older messages, say) leaves the view where it
/// was, and the list stays pinned to its end while items are appended.
#[track_caller]
pub fn lazy_list_keyed<K: Hash>(
    modifier: Modifier,
    state: &LazyListState,
    count: usize,
    key_of: impl Fn(usize) -> K,
    item: impl Fn(usize),
) {
    group(|| {
        // Reading the version makes this recompose when a height is learned.
        let _ = state.version.get();
        let keys: Vec<u64> = (0..count).map(|index| hash_key(key_of(index))).collect();
        let (new_tops, offset, viewport) = {
            let heights = state.inner.heights.borrow();
            let estimate = if heights.is_empty() {
                48.0
            } else {
                state.inner.sum.get() / heights.len() as f32
            };
            let new_tops = place(&keys, &heights, estimate);
            let viewport = match state.scroll.viewport_length() {
                length if length > 0.0 => length,
                _ => 1200.0,
            };
            let mut placed = state.inner.placed.borrow_mut();
            let wanted = Reposition {
                offset: state.scroll.offset(),
                viewport,
                old_viewport: placed.viewport,
                stick_to_end: state.inner.stick_to_end.get(),
                // Aligning to the viewport needs its real size, known after the first layout.
                request: if state.scroll.viewport_length() > 0.0 {
                    state.inner.request.get()
                } else {
                    None
                },
                old_keys: &placed.keys,
                old_tops: &placed.tops,
                new_keys: &keys,
                new_tops: &new_tops,
            }
            .offset();
            placed.keys.clone_from(&keys);
            placed.tops.clone_from(&new_tops);
            placed.viewport = viewport;
            // A request is kept until the item it aims at has been measured, so
            // that it lands right even though the items around it were guesses.
            if let Some(request) = state.inner.request.get() {
                let target = match request {
                    Request::Item(index, _) => index,
                    Request::End => usize::MAX,
                };
                let done = keys
                    .get(target.min(keys.len().saturating_sub(1)))
                    .is_none_or(|key| heights.contains_key(key));
                if done && state.scroll.viewport_length() > 0.0 {
                    state.inner.request.set(None);
                }
            }
            (new_tops, wanted, viewport)
        };
        if (offset - state.scroll.offset()).abs() > 0.05 {
            state.scroll.jump_to(offset);
        }

        let first = item_at(&new_tops, offset).min(count);
        let last = new_tops
            .partition_point(|top| *top < offset + viewport)
            .min(count);
        let first = first.saturating_sub(1);
        let last = (last + 1).min(count);

        column_with(modifier.vertical_scroll(&state.scroll), || {
            spacer(Modifier::new().height(new_tops[first]));
            for (index, &item_key) in keys.iter().enumerate().take(last).skip(first) {
                let learn = state.clone();
                key(item_key, || {
                    stack_with(
                        Modifier::new()
                            .fill_max_width()
                            .on_size_changed(move |size| learn.record(item_key, size.height)),
                        || item(index),
                    );
                });
            }
            spacer(Modifier::new().height(new_tops[count] - new_tops[last]));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heights(pairs: &[(u64, f32)]) -> HashMap<u64, f32> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn items_are_placed_by_measured_height_or_the_estimate() {
        let tops = place(&[1, 2, 3], &heights(&[(1, 10.0), (3, 30.0)]), 20.0);
        assert_eq!(tops, vec![0.0, 10.0, 30.0, 60.0]);
        assert_eq!(item_at(&tops, 0.0), 0);
        assert_eq!(item_at(&tops, 29.9), 1);
        assert_eq!(item_at(&tops, 30.0), 2);
    }

    fn reposition<'a>(
        offset: f32,
        old: (&'a [u64], &'a [f32]),
        new: (&'a [u64], &'a [f32]),
    ) -> Reposition<'a> {
        Reposition {
            offset,
            viewport: 100.0,
            old_viewport: 100.0,
            stick_to_end: false,
            request: None,
            old_keys: old.0,
            old_tops: old.1,
            new_keys: new.0,
            new_tops: new.1,
        }
    }

    #[test]
    fn the_visible_item_stays_when_items_above_are_measured() {
        // Item 2 is in view, 5 px below its top; the two above turn out taller.
        let keys = [1, 2, 3, 4];
        let old = [0.0, 50.0, 100.0, 150.0, 200.0];
        let new = [0.0, 80.0, 190.0, 240.0, 290.0];
        let offset = reposition(105.0, (&keys, &old), (&keys, &new)).offset();
        assert_eq!(offset, 195.0);
    }

    #[test]
    fn the_visible_item_stays_when_items_are_inserted_above() {
        let old_keys = [2, 3];
        let old = [0.0, 40.0, 80.0];
        // Two older items, 50 px each, arrive at the front.
        let new_keys = [0, 1, 2, 3];
        let new = [0.0, 50.0, 100.0, 140.0, 180.0];
        let offset = reposition(45.0, (&old_keys, &old), (&new_keys, &new)).offset();
        assert_eq!(offset, 145.0);
    }

    #[test]
    fn a_pinned_list_follows_its_end_until_the_user_scrolls_away() {
        let old_keys = [1, 2, 3];
        let old = [0.0, 60.0, 120.0, 180.0];
        let new_keys = [1, 2, 3, 4];
        let new = [0.0, 60.0, 120.0, 180.0, 240.0];
        let mut at_end = reposition(80.0, (&old_keys, &old), (&new_keys, &new));
        at_end.stick_to_end = true;
        assert_eq!(at_end.offset(), 140.0);
        let mut scrolled_up = reposition(30.0, (&old_keys, &old), (&new_keys, &new));
        scrolled_up.stick_to_end = true;
        assert_eq!(scrolled_up.offset(), 30.0);
    }

    #[test]
    fn a_pinned_list_stays_pinned_when_the_viewport_shrinks() {
        let keys = [1, 2, 3];
        let tops = [0.0, 60.0, 120.0, 180.0];
        let mut list = reposition(80.0, (&keys, &tops), (&keys, &tops));
        list.stick_to_end = true;
        list.viewport = 50.0;
        assert_eq!(list.offset(), 130.0);
    }

    #[test]
    fn a_new_pinned_list_starts_at_its_end() {
        let new_keys = [1, 2, 3];
        let new = [0.0, 60.0, 120.0, 180.0];
        let mut list = reposition(0.0, (&[], &[]), (&new_keys, &new));
        list.stick_to_end = true;
        assert_eq!(list.offset(), 80.0);
    }

    #[test]
    fn requests_align_the_item() {
        let keys = [1, 2, 3, 4];
        let tops = [0.0, 60.0, 120.0, 180.0, 240.0];
        let mut list = reposition(0.0, (&keys, &tops), (&keys, &tops));
        list.request = Some(Request::Item(2, ItemAlign::Start));
        assert_eq!(list.offset(), 120.0);
        list.request = Some(Request::Item(2, ItemAlign::Center));
        assert_eq!(list.offset(), 100.0);
        list.request = Some(Request::Item(1, ItemAlign::End));
        assert_eq!(list.offset(), 20.0);
        list.request = Some(Request::Item(9, ItemAlign::Start));
        assert_eq!(list.offset(), 140.0);
        list.request = Some(Request::End);
        assert_eq!(list.offset(), 140.0);
    }
}
