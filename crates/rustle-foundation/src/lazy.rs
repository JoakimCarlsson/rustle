use rustle_runtime::{group, key};
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

/// What a variable-height lazy list remembers: its scroll position and the
/// heights its items turned out to have.
#[derive(Clone)]
pub struct LazyListState {
    scroll: ScrollState,
    heights: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<usize, f32>>>,
    /// Bumped when a height is learned, so the list composes again with it.
    version: rustle_runtime::State<u64>,
}

impl LazyListState {
    /// The scroll state, for reading the offset or scrolling.
    pub fn scroll(&self) -> &ScrollState {
        &self.scroll
    }

    fn record(&self, index: usize, height: f32) {
        let changed = {
            let mut heights = self.heights.borrow_mut();
            let old = heights.insert(index, height);
            old.is_none_or(|old| (old - height).abs() > 0.5)
        };
        if changed {
            self.version.update(|version| *version += 1);
        }
    }
}

/// Remembers a [`LazyListState`] at this call site.
#[track_caller]
pub fn remember_lazy_list_state() -> LazyListState {
    group(|| LazyListState {
        scroll: remember_scroll_state(),
        heights: rustle_runtime::remember(std::rc::Rc::default).get(),
        version: rustle_runtime::remember(|| 0),
    })
}

/// Emits a vertically scrolling list of `count` items of any heights,
/// composing only the items near the viewport. `item` emits the item at an index.
///
/// Heights are learned as items are laid out; until then an item counts as the
/// average of those known, so the scrollbar-length of the list is an estimate
/// that settles as it is scrolled.
#[track_caller]
pub fn lazy_list(modifier: Modifier, state: &LazyListState, count: usize, item: impl Fn(usize)) {
    group(|| {
        // Reading the version makes this recompose when a height is learned.
        let _ = state.version.get();
        let heights = state.heights.borrow().clone();
        let estimate = if heights.is_empty() {
            48.0
        } else {
            heights.values().sum::<f32>() / heights.len() as f32
        };
        let height_of = |index: usize| heights.get(&index).copied().unwrap_or(estimate);

        let viewport = match state.scroll.viewport_length() {
            length if length > 0.0 => length,
            _ => 1200.0,
        };
        let (top, bottom) = (state.scroll.offset(), state.scroll.offset() + viewport);
        let (mut first, mut last, mut y) = (count, count, 0.0_f32);
        let mut tops = Vec::with_capacity(count + 1);
        for index in 0..count {
            tops.push(y);
            let height = height_of(index);
            if first == count && y + height > top {
                first = index;
            }
            if first != count && last == count && y > bottom {
                last = index;
            }
            y += height;
        }
        tops.push(y);
        let first = first.saturating_sub(1).min(count);
        let last = (last + 1).min(count);

        column_with(modifier.vertical_scroll(&state.scroll), || {
            spacer(Modifier::new().height(tops[first]));
            for index in first..last {
                let learn = state.clone();
                key(index, || {
                    stack_with(
                        Modifier::new()
                            .fill_max_width()
                            .on_size_changed(move |size| learn.record(index, size.height)),
                        || item(index),
                    );
                });
            }
            spacer(Modifier::new().height(tops[count] - tops[last]));
        });
    });
}
