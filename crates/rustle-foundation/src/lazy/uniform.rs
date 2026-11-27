//! Lazy lists whose items take whole rows of one fixed height, like gpui's `uniform_list`:
//! what is in view is worked out from the scroll offset by arithmetic, with no key hashed
//! and no height learned or kept for items that are not composed.

use std::hash::Hash;

use rustle_runtime::{group, key, untracked};
use rustle_ui::Modifier;

use super::{
    COMPOSED_MARGIN, END_TOLERANCE, ItemAlign, LazyContent, LazyListItemInfo, LazyListLayoutInfo,
    LazyListState, Placed, Request,
};
use crate::linear::{column_with, spacer, stack_with};

/// How the items of a uniform list lie on its rows, all of one height: each item takes a
/// whole number of rows, one after the other, and an item may take none, which hides it.
///
/// Both lookups are asked only for the few items and rows near the viewport each time the
/// list composes, so they should be cheap, as a prefix sum over the rows of each item is.
pub trait UniformRows {
    /// How many items the list has.
    fn items(&self) -> usize;

    /// How many rows the items take together.
    fn rows(&self) -> usize;

    /// The first row of the item at `item`; for `item` at or past [`UniformRows::items`],
    /// [`UniformRows::rows`].
    fn first_row(&self, item: usize) -> usize;

    /// The item drawn in `row`; for `row` at or past [`UniformRows::rows`],
    /// [`UniformRows::items`].
    fn item_at_row(&self, row: usize) -> usize;

    /// Whether every item takes exactly one row, so the list can tell any item's row
    /// without asking; false unless an implementation knows it is so.
    fn one_per_row(&self) -> bool {
        false
    }
}

/// `count` items of one row each, the rows of [`uniform_list`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneRowEach(pub usize);

impl UniformRows for OneRowEach {
    /// The count of items.
    fn items(&self) -> usize {
        self.0
    }

    /// The count of items, one row each.
    fn rows(&self) -> usize {
        self.0
    }

    /// The item's index, at most the count.
    fn first_row(&self, item: usize) -> usize {
        item.min(self.0)
    }

    /// The row's index, at most the count.
    fn item_at_row(&self, row: usize) -> usize {
        row.min(self.0)
    }

    /// Always: each item is one row.
    fn one_per_row(&self) -> bool {
        true
    }
}

/// The shortest row a uniform list takes, so that a row height of zero cannot divide by it.
const MIN_ROW: f32 = 1.0e-3;

/// What a uniform list placed when it last composed, which the [`LazyListState`] it scrolls
/// answers from: where the composed items start, and enough to work out the others.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Uniform {
    /// How many items the list has.
    count: usize,
    /// How many rows they take.
    rows: usize,
    /// How tall a row is, in logical pixels.
    row_height: f32,
    /// The padding before the first row and after the last.
    padding: (f32, f32),
    /// How long the viewport was.
    viewport: f32,
    /// Whether every item takes one row, so items and rows are the same.
    one_per_row: bool,
    /// The first item composed.
    first: usize,
    /// The first row of each item composed and of the one after the last.
    starts: Vec<usize>,
}

impl Uniform {
    /// The index one past the last item composed.
    fn last(&self) -> usize {
        self.first + self.starts.len().saturating_sub(1)
    }

    /// Where `row` starts, from the start of the list.
    fn top(&self, row: usize) -> f32 {
        self.padding.0 + row as f32 * self.row_height
    }

    /// How long the list is with its padding.
    fn length(&self) -> f32 {
        self.top(self.rows) + self.padding.1
    }

    /// The first row of `item`: exact for the items composed and when every item takes one
    /// row, otherwise counted on from the nearest composed item at one row an item.
    fn first_row(&self, item: usize) -> usize {
        if item >= self.count {
            return self.rows;
        }
        if self.one_per_row {
            return item;
        }
        let (Some(&start), Some(&end)) = (self.starts.first(), self.starts.last()) else {
            return item.min(self.rows);
        };
        let last = self.last();
        if item < self.first {
            start.saturating_sub(self.first - item)
        } else if item > last {
            (end + (item - last)).min(self.rows)
        } else {
            self.starts[item - self.first]
        }
    }

    /// The item drawn in `row`, worked out as [`Uniform::first_row`] works rows out.
    fn item_at_row(&self, row: usize) -> usize {
        if row >= self.rows {
            return self.count;
        }
        if self.one_per_row {
            return row;
        }
        let (Some(&start), Some(&end)) = (self.starts.first(), self.starts.last()) else {
            return row.min(self.count);
        };
        if row < start {
            self.first.saturating_sub(start - row)
        } else if row >= end {
            (self.last() + (row - end)).min(self.count)
        } else {
            self.first + self.starts.partition_point(|first| *first <= row) - 1
        }
    }

    /// The item at `offset` along the list, which is the last one past its end, or `None`
    /// for a list with no items.
    fn item_at(&self, offset: f32) -> Option<usize> {
        if self.count == 0 {
            return None;
        }
        let row = ((offset - self.padding.0) / self.row_height)
            .floor()
            .max(0.0) as usize;
        let mut item = self.item_at_row(row).min(self.count - 1);
        while item + 1 < self.count && self.first_row(item + 1) <= row {
            item += 1;
        }
        Some(item)
    }

    /// Where `item` starts along the list.
    fn item_top(&self, item: usize) -> f32 {
        self.top(self.first_row(item))
    }

    /// The first item in view when the list is scrolled to `offset`.
    pub(super) fn first_visible_item(&self, offset: f32) -> Option<usize> {
        self.item_at(offset)
    }

    /// How far `offset` is past the start of the first item in view.
    pub(super) fn first_visible_item_scroll_offset(&self, offset: f32) -> f32 {
        self.item_at(offset)
            .map_or(0.0, |item| (offset - self.item_top(item)).max(0.0))
    }

    /// The items in view when the list is scrolled to `offset`, as
    /// [`LazyListState::layout_info`] gives them.
    pub(super) fn layout_info(&self, offset: f32) -> LazyListLayoutInfo {
        let end = offset + self.viewport;
        let mut visible_items_info = Vec::new();
        if let Some(first) = self.item_at(offset) {
            for index in first..self.count {
                let (top, bottom) = (self.item_top(index), self.item_top(index + 1));
                if top >= end {
                    break;
                }
                if bottom > top || index == first {
                    visible_items_info.push(LazyListItemInfo {
                        index,
                        offset: top - offset,
                        size: bottom - top,
                    });
                }
            }
        }
        LazyListLayoutInfo {
            visible_items_info,
            viewport_start_offset: 0.0,
            viewport_end_offset: self.viewport,
            total_items_count: self.count,
        }
    }
}

/// Emits a vertically scrolling list of `count` items, each `item_height` tall, scrolled by
/// `state`, composing only the items in view and a small margin around them. `item` emits the
/// item at an index, in a cell `item_height` tall that fills the list across.
///
/// Unlike [`lazy_list_with`](super::lazy_list_with), nothing is learned of the items: the
/// items in view, the length of the list and where to scroll for
/// [`LazyListState::scroll_to_item`] are worked out from `item_height` alone, in constant
/// time however long the list is, and no key is hashed for items not composed. `content`
/// gives the padding at both ends; its spacing, item animation and reversed layout are not
/// used. Items are told apart by their index, and are not pinned while out of view. The
/// queries of `state`, such as [`LazyListState::first_visible_item`], answer for this list.
#[track_caller]
pub fn uniform_list(
    modifier: Modifier,
    state: &LazyListState,
    content: LazyContent,
    count: usize,
    item_height: f32,
    item: impl Fn(usize),
) {
    uniform_list_with(
        modifier,
        state,
        content,
        item_height,
        &OneRowEach(count),
        |index| index,
        item,
    );
}

/// Like [`uniform_list`], with items that take a whole number of rows `row_height` tall
/// each, as `rows` lays them out, such as the lines of a text as they wrap, and that are
/// told apart by `key_of` rather than their index. `key_of` is asked only of the items
/// composed. `item` emits the item at an index, in a cell as tall as its rows.
///
/// The rows of the items composed are kept in `state`, so its queries are exact for the
/// items near the viewport and, away from them, count one row an item until the list
/// composes there.
#[track_caller]
pub fn uniform_list_with<K: Hash>(
    modifier: Modifier,
    state: &LazyListState,
    content: LazyContent,
    row_height: f32,
    rows: &impl UniformRows,
    key_of: impl Fn(usize) -> K,
    item: impl Fn(usize),
) {
    group(|| {
        let _ = state.version.get();
        state.watch_scroll();
        state.inner.uncovered.set(false);
        let measured = untracked(|| state.scroll.viewport_length());
        state.inner.composed_viewport.set(Some(measured));
        let placed = Placement::new(rows, row_height, &content, measured);
        let offset = placed.settle(state, rows);
        let window = placed.window(rows, offset);
        let covered = (
            if window.first == 0 {
                f32::NEG_INFINITY
            } else {
                placed.top(window.starts[0])
            },
            if window.last == placed.count {
                f32::INFINITY
            } else {
                placed.top(window.starts[window.starts.len() - 1])
            },
        );
        state.inner.covered.set(covered);
        {
            let mut kept = state.inner.placed.borrow_mut();
            if !kept.keys.is_empty() {
                *kept = Placed::default();
            }
        }
        if !state.inner.grid_rows.borrow().is_empty() {
            state.inner.grid_rows.borrow_mut().clear();
        }
        let starts = window.starts.clone();
        *state.inner.uniform.borrow_mut() = Some(Uniform {
            count: placed.count,
            rows: placed.rows,
            row_height: placed.row_height,
            padding: placed.padding,
            viewport: placed.viewport,
            one_per_row: rows.one_per_row(),
            first: window.first,
            starts: window.starts,
        });
        column_with(modifier.vertical_scroll_with(&state.scroll, false), || {
            spacer(Modifier::new().height(placed.top(starts[0])));
            for (place, index) in (window.first..window.last).enumerate() {
                let height = (starts[place + 1] - starts[place]) as f32 * placed.row_height;
                key(key_of(index), || {
                    stack_with(Modifier::new().fill_max_width().height(height), || {
                        item(index)
                    });
                });
            }
            let end = placed.top(starts[starts.len() - 1]);
            spacer(Modifier::new().height(placed.length() - end));
        });
    });
}

/// The size of a uniform list as it composes: its items, rows, padding and viewport.
struct Placement {
    /// How many items there are.
    count: usize,
    /// How many rows they take.
    rows: usize,
    /// How tall a row is.
    row_height: f32,
    /// The padding before the first row and after the last.
    padding: (f32, f32),
    /// The viewport's length, or a generous guess before layout measured it.
    viewport: f32,
    /// Whether layout has measured the viewport.
    measured: bool,
}

/// The items a uniform list composes: from `first` up to `last`, each starting at the row
/// in `starts`, which ends with the row after the last.
struct Window {
    /// The first item composed.
    first: usize,
    /// One past the last item composed.
    last: usize,
    /// The first row of each item composed and of the one after.
    starts: Vec<usize>,
}

impl Placement {
    /// The placement of `rows`, each row `row_height` tall, with the padding of `content`,
    /// in a viewport layout last measured as `measured` long.
    fn new(rows: &impl UniformRows, row_height: f32, content: &LazyContent, measured: f32) -> Self {
        Self {
            count: rows.items(),
            rows: rows.rows(),
            row_height: row_height.max(MIN_ROW),
            padding: (content.padding_start, content.padding_end),
            viewport: if measured > 0.0 { measured } else { 1200.0 },
            measured: measured > 0.0,
        }
    }

    /// Where `row` starts, from the start of the list.
    fn top(&self, row: usize) -> f32 {
        self.padding.0 + row as f32 * self.row_height
    }

    /// How long the list is with its padding.
    fn length(&self) -> f32 {
        self.top(self.rows) + self.padding.1
    }

    /// The furthest the list scrolls.
    fn max_offset(&self) -> f32 {
        (self.length() - self.viewport).max(0.0)
    }

    /// Scrolls `state` as the request it holds asks, or to the end when it follows its end
    /// and was there, and returns the offset the list is at.
    fn settle(&self, state: &LazyListState, rows: &impl UniformRows) -> f32 {
        let scrolled = untracked(|| state.scroll.offset());
        let was_at_end = state.inner.uniform.borrow().as_ref().is_none_or(|before| {
            scrolled >= (before.length() - before.viewport).max(0.0) - END_TOLERANCE
        });
        let request = if self.measured {
            state.inner.request.take()
        } else {
            None
        };
        let item_top = |index: usize| self.top(rows.first_row(index));
        let wanted = match request {
            Some(Request::At(index, extra)) => Some(item_top(index.min(self.count)) + extra),
            Some(Request::Item(index, align)) => {
                let index = index.min(self.count.saturating_sub(1));
                let (top, bottom) = (item_top(index), item_top(index + 1));
                Some(match align {
                    ItemAlign::Start => top,
                    ItemAlign::Center => top - (self.viewport - (bottom - top)) / 2.0,
                    ItemAlign::End => bottom - self.viewport,
                })
            }
            Some(Request::End) => Some(self.max_offset()),
            None if state.inner.stick_to_end.get() && self.measured && was_at_end => {
                Some(self.max_offset())
            }
            None => None,
        };
        let Some(wanted) = wanted else {
            return scrolled;
        };
        let offset = if self.measured {
            wanted.clamp(0.0, self.max_offset())
        } else {
            wanted.max(0.0)
        };
        if (offset - scrolled).abs() > 0.05 {
            state.scroll.jump_to(offset);
        }
        offset
    }

    /// The items to compose with the list at `offset`: those in view and those within a
    /// quarter of the viewport of it.
    fn window(&self, rows: &impl UniformRows, offset: f32) -> Window {
        let margin = self.viewport * COMPOSED_MARGIN;
        let row_at = |at: f32| ((at - self.padding.0) / self.row_height).floor().max(0.0) as usize;
        let first_row = row_at(offset - margin);
        let end_row = row_at(offset + self.viewport + margin);
        let first = rows.item_at_row(first_row.min(self.rows)).min(self.count);
        let last = if end_row >= self.rows {
            self.count
        } else {
            (rows.item_at_row(end_row) + 1).min(self.count)
        };
        let last = last.max(first);
        let starts = (first..=last).map(|index| rows.first_row(index)).collect();
        Window {
            first,
            last,
            starts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A list of 100 items of two rows each, 10 tall, with items 40 to 50 composed.
    fn doubled() -> Uniform {
        Uniform {
            count: 100,
            rows: 200,
            row_height: 10.0,
            padding: (4.0, 0.0),
            viewport: 100.0,
            one_per_row: false,
            first: 40,
            starts: (40..=50).map(|item| item * 2).collect(),
        }
    }

    /// Rows and items are exact over the items composed and counted on beyond them.
    #[test]
    fn rows_are_exact_where_composed_and_counted_on_elsewhere() {
        let placed = doubled();
        assert_eq!(placed.first_row(45), 90);
        assert_eq!(placed.item_at_row(91), 45);
        assert_eq!(placed.first_row(38), 78);
        assert_eq!(placed.item_at_row(78), 38);
        assert_eq!(placed.first_row(52), 102);
        assert_eq!(placed.first_row(100), 200);
        assert_eq!(placed.item_at_row(200), 100);
        assert_eq!(placed.item_at(4.0 + 905.0), Some(45));
        assert_eq!(placed.item_at(1.0e9), Some(99));
        assert_eq!(Uniform::default().item_at(0.0), None);
    }

    /// One row an item maps items and rows one to one.
    #[test]
    fn one_row_each_maps_items_to_rows() {
        let rows = OneRowEach(5);
        assert_eq!((rows.items(), rows.rows()), (5, 5));
        assert_eq!(rows.first_row(7), 5);
        assert_eq!(rows.item_at_row(3), 3);
    }
}
