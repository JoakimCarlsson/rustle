use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;
use std::time::Duration;

use rustle_animation::{AnimationSpec, remember_animatable};
use rustle_geometry::Size;
use rustle_runtime::{State, frame_clock, group, key, launched_effect, remember};
use rustle_ui::{
    LOCAL_PINNABLE_CONTAINER, Modifier, PinnableContainer, PinnedHandle, ScrollState,
    remember_scroll_state,
};

use crate::linear::{column_with, row_with, spacer, stack_with};

mod uniform;

pub use uniform::{OneRowEach, UniformRows, uniform_list, uniform_list_with};

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
    /// Item `.0` at the start of the viewport, scrolled `.1` pixels further.
    At(usize, f32),
    End,
}

/// Where the items of a list were when it last composed, kept from one composition to
/// the next and placed afresh only when the items, the padding or the estimate for items
/// not measured change, or a length learned could not be taken in during layout.
#[derive(Default)]
struct Placed {
    /// The key of each item.
    keys: Vec<u64>,
    /// The top of each item and, last, the end of the list.
    tops: Vec<f32>,
    /// How tall the viewport was.
    viewport: f32,
    /// The length of each item, measured or estimated, summed in a Fenwick tree.
    lengths: Lengths,
    /// Whether each item's length was measured rather than estimated.
    measured: Vec<bool>,
    /// The index of each item, by key.
    index: KeyMap<usize>,
    /// The length counted for items not measured.
    estimate: f32,
    /// The padding before the first item and after the last.
    padding: (f32, f32),
    /// Lengths learned since the list composed that the next composition places, by index.
    pending: Vec<(usize, f32)>,
    /// The end of the items the latest composition composed around the viewport.
    last: usize,
    /// Whether two items share a key, so lengths are looked up by key every composition.
    duplicates: bool,
    /// Whether a length learned at or past the item at the start of the viewport is taken
    /// in during layout rather than by composing again.
    remeasure_in_place: bool,
    /// Whether the list is laid out from its end, its offset counted from there, so an item
    /// grows away from the offset's origin at its near side.
    reverse: bool,
}

/// What [`Placed::remeasure`] did with a length learned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Remeasured {
    /// The item is not among those placed: the next composition finds the length by key.
    Unplaced,
    /// The length waits for the next composition, which keeps the view where it was.
    Deferred,
    /// The length was taken in where the list is, and what was composed still covers the
    /// viewport.
    Absorbed,
    /// The length was taken in, but the items composed no longer cover the viewport.
    Uncovered,
}

impl Placed {
    /// Places `keys` with `estimate` for items not measured and `padding` at both ends,
    /// reusing what was placed when nothing changed. Lengths are looked up in `heights`
    /// only when the keys changed or `rehash` asks for it.
    ///
    /// Returns the keys and tops placed before, with whether each was measured, when the
    /// items were placed afresh, or `None` when what was placed stands as it was.
    fn update(
        &mut self,
        keys: Vec<u64>,
        heights: &KeyMap<f32>,
        estimate: f32,
        padding: (f32, f32),
        rehash: bool,
    ) -> Option<(Vec<u64>, Vec<f32>, Vec<bool>)> {
        let same = !rehash && !self.duplicates && !self.tops.is_empty() && self.keys == keys;
        let estimate_moved = estimate.to_bits() != self.estimate.to_bits()
            && self.measured.iter().any(|measured| !measured);
        if same && self.pending.is_empty() && padding == self.padding && !estimate_moved {
            return None;
        }
        let old_tops = std::mem::take(&mut self.tops);
        let old_measured = self.measured.clone();
        let mut values = std::mem::take(&mut self.lengths).values;
        let old_keys = if same {
            for (index, length) in std::mem::take(&mut self.pending) {
                values[index] = length;
                self.measured[index] = true;
            }
            for (value, measured) in values.iter_mut().zip(&self.measured) {
                if !measured {
                    *value = estimate;
                }
            }
            keys
        } else {
            self.pending.clear();
            self.measured.clear();
            values.clear();
            self.index.clear();
            for (position, key) in keys.iter().enumerate() {
                let height = heights.get(key).copied();
                self.measured.push(height.is_some());
                values.push(height.unwrap_or(estimate));
                self.index.insert(*key, position);
            }
            self.duplicates = self.index.len() != keys.len();
            std::mem::replace(&mut self.keys, keys)
        };
        let mut tops = Vec::with_capacity(values.len() + 1);
        let mut y = padding.0;
        for value in &values {
            tops.push(y);
            y += value;
        }
        tops.push(y + padding.1);
        self.tops = tops;
        self.lengths = Lengths::new(values);
        self.estimate = estimate;
        self.padding = padding;
        Some((old_keys, old_tops, old_measured))
    }

    /// Takes in that the item with `key` is now `height` long, while the list is scrolled
    /// to `offset`, with `request` pending and following its end if `stick_to_end`.
    ///
    /// A length at or past the item at the start of the viewport, which moves nothing in
    /// view, is taken in at once: the item's length in the tree, the tops after it and the
    /// stretch `covered` are moved by the difference. Any other waits in `pending` for the
    /// next composition, which keeps the view where it was.
    fn remeasure(
        &mut self,
        key: u64,
        height: f32,
        offset: f32,
        follow: (bool, bool),
        covered: &Cell<(f32, f32)>,
    ) -> Remeasured {
        let (stick_to_end, requested) = follow;
        let Some(&index) = self.index.get(&key) else {
            return Remeasured::Unplaced;
        };
        let count = self.keys.len();
        if index >= count || self.keys[index] != key {
            return Remeasured::Unplaced;
        }
        let delta = height - self.lengths.get(index);
        if !self.remeasure_in_place
            || requested
            || self.duplicates
            || self.pending.iter().any(|(waiting, _)| *waiting == index)
            || !self.stays_in_view(index, delta, offset, stick_to_end)
        {
            self.pending.push((index, height));
            return Remeasured::Deferred;
        }
        self.lengths.set(index, height);
        self.measured[index] = true;
        for top in &mut self.tops[index + 1..] {
            *top += delta;
        }
        let (from, mut to) = covered.get();
        if index < self.last && to.is_finite() {
            to += delta;
            covered.set((from, to));
        }
        if offset + self.viewport > to {
            Remeasured::Uncovered
        } else {
            Remeasured::Absorbed
        }
    }

    /// Whether item `index` growing by `delta` leaves the list at `offset` where the next
    /// composition would keep it: the item is at or past the one at the start of the
    /// viewport, and a list following its end stays at it. In a list laid out from its end
    /// and scrolled away from it, an item grows at the side nearer the end, so one at or
    /// before the item at the start of the viewport would move what is in view: that waits
    /// for the next composition, which keeps what is in view where it was.
    fn stays_in_view(&self, index: usize, delta: f32, offset: f32, stick_to_end: bool) -> bool {
        let Some(&old_total) = self.tops.last() else {
            return false;
        };
        let new_max = (old_total + delta - self.viewport).max(0.0);
        if offset > new_max + 0.05 {
            return false;
        }
        let old_end = (old_total - self.viewport).max(0.0);
        if stick_to_end && offset >= old_end - END_TOLERANCE {
            return (new_max - offset).abs() <= 0.05;
        }
        if offset <= 0.0 {
            return true;
        }
        let top = self
            .lengths
            .index_at(offset - self.padding.0)
            .min(self.keys.len().saturating_sub(1));
        if self.reverse {
            return index > top;
        }
        index >= steady_anchor(top, offset + self.viewport, &self.tops, &self.measured)
    }
}

/// The lengths of a list's items in a Fenwick tree, so one length changes, and the sum
/// of those before an item or the item at a distance is found, in logarithmic time.
#[derive(Default)]
struct Lengths {
    /// The length of each item.
    values: Vec<f32>,
    /// The tree, one-based: entry `i` sums the `i & -i` lengths ending with item `i - 1`.
    tree: Vec<f32>,
}

impl Lengths {
    /// The tree over `values`, built in linear time.
    fn new(values: Vec<f32>) -> Self {
        let count = values.len();
        let mut tree = vec![0.0; count + 1];
        for position in 1..=count {
            tree[position] += values[position - 1];
            let parent = position + position.isolate_lowest_one();
            if parent <= count {
                tree[parent] += tree[position];
            }
        }
        Self { values, tree }
    }

    /// The length of item `index`.
    ///
    /// # Panics
    ///
    /// If `index` is past the last item.
    fn get(&self, index: usize) -> f32 {
        self.values[index]
    }

    /// Makes item `index` `length` long.
    ///
    /// # Panics
    ///
    /// If `index` is past the last item.
    fn set(&mut self, index: usize, length: f32) {
        let delta = length - self.values[index];
        self.values[index] = length;
        let mut position = index + 1;
        while position < self.tree.len() {
            self.tree[position] += delta;
            position += position.isolate_lowest_one();
        }
    }

    /// The sum of the lengths of the items before `end`.
    #[cfg(test)]
    fn prefix(&self, end: usize) -> f32 {
        let mut position = end.min(self.values.len());
        let mut sum = 0.0;
        while position > 0 {
            sum += self.tree[position];
            position &= position - 1;
        }
        sum
    }

    /// The index of the item `distance` from the start of the first, which is the count
    /// of items when it is at or past the end: the last `i` whose items before it sum to
    /// no more than `distance`.
    fn index_at(&self, distance: f32) -> usize {
        if distance < 0.0 {
            return 0;
        }
        let count = self.values.len();
        let mut position = 0;
        let mut left = distance;
        let mut step = count.next_power_of_two();
        while step > 0 {
            let next = position + step;
            if next <= count && self.tree[next] <= left {
                position = next;
                left -= self.tree[next];
            }
            step >>= 1;
        }
        position
    }
}

struct ListInner {
    /// The measured height of each item, by key.
    heights: RefCell<KeyMap<f32>>,
    /// The sum of `heights`, for the average that estimates the rest.
    sum: Cell<f32>,
    placed: RefCell<Placed>,
    /// Whether the next composition places every item afresh by key, as a length was
    /// learned while what was placed could not be reached.
    rehash: Cell<bool>,
    /// The length of the viewport, as layout last measured it, when the variable-size list
    /// last composed: a resize composes the list again, while a change of the content's
    /// length alone, which the same scroll state holds, does not.
    composed_viewport: Cell<Option<f32>>,
    stick_to_end: Cell<bool>,
    request: Cell<Option<Request>>,
    /// How many lanes the staggered grid had and the lane of each item, by key.
    lanes: RefCell<(usize, KeyMap<usize>)>,
    /// Until when items slide to their new places after a reorder.
    slide_until: Cell<Option<Duration>>,
    /// Where each item of a staggered grid was placed when it last composed, by key, and
    /// the keys it had, so the view can be held and reorders noticed.
    stagger: RefCell<(Vec<u64>, KeyMap<f32>)>,
    /// For a spanned grid, the items of each row, so items can be asked for by index.
    grid_rows: RefCell<Vec<Vec<usize>>>,
    /// How many handles pin each item, by key: pinned items stay composed out of view.
    pinned: RefCell<KeyMap<usize>>,
    /// The stretch of the list, from the start of its first composed item to the end of
    /// its last, that its latest composition covers; a scroll that keeps the viewport
    /// inside it composes nothing.
    covered: Cell<(f32, f32)>,
    /// Whether the list asked to compose again because the viewport left what it covers,
    /// and has not composed since.
    uncovered: Cell<bool>,
    /// The watch on scrolls that asks the list to compose again once the viewport leaves
    /// what it covers, kept while the list is.
    watch: RefCell<Option<rustle_runtime::ObserverHandle>>,
    /// What a uniform list placed when it last composed, while one scrolls by this state
    /// rather than a list that measures its items.
    uniform: RefCell<Option<uniform::Uniform>>,
}

/// One item a lazy list laid out in view, like Compose's `LazyListItemInfo`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LazyListItemInfo {
    /// The item's index in the list.
    pub index: usize,
    /// Where the item starts, along the list, from the start of the viewport, in
    /// logical pixels; negative while it is partly scrolled out.
    pub offset: f32,
    /// How long the item is along the list, in logical pixels.
    pub size: f32,
}

/// What a lazy list laid out in its latest composition, like Compose's
/// `LazyListLayoutInfo`. See [`LazyListState::layout_info`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LazyListLayoutInfo {
    /// The items in view, first to last.
    pub visible_items_info: Vec<LazyListItemInfo>,
    /// Where the viewport starts, in logical pixels.
    pub viewport_start_offset: f32,
    /// Where the viewport ends, in logical pixels.
    pub viewport_end_offset: f32,
    /// How many items the list has.
    pub total_items_count: usize,
}

/// What a variable-height lazy list remembers: its scroll position and the
/// heights its items turned out to have.
///
/// Like Compose's `LazyListState(...)`, it can be made outside composition with
/// [`LazyListState::new`], by a state holder that owns it, or remembered at a call site
/// with [`remember_lazy_list_state`]. Two handles are equal when they are the same state.
#[derive(Clone)]
pub struct LazyListState {
    scroll: ScrollState,
    inner: Rc<ListInner>,
    /// Bumped when a height is learned, so the list composes again with it.
    version: State<u64>,
}

impl PartialEq for LazyListState {
    /// Whether both handles are the same state.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl std::fmt::Debug for LazyListState {
    /// The scroll state.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LazyListState")
            .field("scroll", &self.scroll)
            .finish()
    }
}

impl Default for LazyListState {
    /// A list scrolled to its start.
    fn default() -> Self {
        Self::new(0, 0.0)
    }
}

impl ListInner {
    /// What a list knows before it was laid out: nothing.
    fn new() -> Self {
        Self {
            heights: RefCell::default(),
            sum: Cell::new(0.0),
            placed: RefCell::default(),
            rehash: Cell::new(false),
            composed_viewport: Cell::new(None),
            stick_to_end: Cell::new(false),
            request: Cell::new(None),
            lanes: RefCell::default(),
            slide_until: Cell::new(None),
            stagger: RefCell::default(),
            grid_rows: RefCell::default(),
            pinned: RefCell::default(),
            covered: Cell::new((f32::NEG_INFINITY, f32::INFINITY)),
            uncovered: Cell::new(false),
            watch: RefCell::default(),
            uniform: RefCell::default(),
        }
    }
}

impl LazyListState {
    /// A list with item `first_visible_item` at the start of its viewport, scrolled
    /// `first_visible_item_offset` logical pixels further, which belongs to no
    /// composition: Compose's `LazyListState(firstVisibleItemIndex,
    /// firstVisibleItemScrollOffset)`, for a state holder to own.
    pub fn new(first_visible_item: usize, first_visible_item_offset: f32) -> Self {
        let inner = ListInner::new();
        if first_visible_item > 0 || first_visible_item_offset > 0.0 {
            inner.request.set(Some(Request::At(
                first_visible_item,
                first_visible_item_offset,
            )));
        }
        Self {
            scroll: ScrollState::new(0.0),
            inner: Rc::new(inner),
            version: rustle_runtime::mutable_state(0),
        }
    }

    /// Scrolls so that item `index` is at the start of the viewport, then `offset`
    /// logical pixels further: Compose's `scrollToItem(index, scrollOffset)`. Done the
    /// next time the list composes.
    pub fn scroll_to_item_offset(&self, index: usize, offset: f32) {
        self.inner.request.set(Some(Request::At(index, offset)));
        self.scroll.show_scrollbar();
        self.bump();
    }

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
    ///
    /// In a [`lazy_vertical_grid_spanned`] the index is that of the item, not of its row.
    pub fn scroll_to_item(&self, index: usize, align: ItemAlign) {
        let rows = self.inner.grid_rows.borrow();
        let index = if rows.is_empty() {
            index
        } else {
            rows.partition_point(|row| row[0] <= index)
                .saturating_sub(1)
        };
        drop(rows);
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

    /// The index of the first item in view as of the latest composition, if any. In a
    /// [`lazy_vertical_grid_spanned`] it is the first item of the first row in view.
    pub fn first_visible_item(&self) -> Option<usize> {
        if let Some(uniform) = self.inner.uniform.borrow().as_ref() {
            return uniform.first_visible_item(self.scroll.offset());
        }
        let placed = self.inner.placed.borrow();
        let count = placed.keys.len();
        let row =
            (count > 0).then(|| item_at(&placed.tops, self.scroll.offset()).min(count - 1))?;
        let rows = self.inner.grid_rows.borrow();
        Some(rows.get(row).map_or(row, |items| items[0]))
    }

    /// What the latest composition laid out: the items in view, where each is and how
    /// long it is along the list, the viewport and how many items there are, like
    /// Compose's `LazyListState.layoutInfo`. Empty until the list has composed. In a
    /// [`lazy_vertical_grid_spanned`] every item of a row in view is listed with the row's
    /// place and length.
    pub fn layout_info(&self) -> LazyListLayoutInfo {
        if let Some(uniform) = self.inner.uniform.borrow().as_ref() {
            return uniform.layout_info(self.scroll.offset());
        }
        let placed = self.inner.placed.borrow();
        let count = placed.keys.len();
        let offset = self.scroll.offset();
        let end = offset + placed.viewport;
        let rows = self.inner.grid_rows.borrow();
        let mut visible_items_info = Vec::new();
        if count > 0 {
            for row in item_at(&placed.tops, offset).min(count - 1)..count {
                let (top, bottom) = (placed.tops[row], placed.tops[row + 1]);
                if top >= end {
                    break;
                }
                let items = rows.get(row).map_or(row..row + 1, |items| {
                    items[0]..items.last().map_or(items[0], |last| last + 1)
                });
                visible_items_info.extend(items.map(|index| LazyListItemInfo {
                    index,
                    offset: top - offset,
                    size: bottom - top,
                }));
            }
        }
        let total_items_count = rows
            .last()
            .and_then(|items| items.last())
            .map_or(count, |last| last + 1);
        LazyListLayoutInfo {
            visible_items_info,
            viewport_start_offset: 0.0,
            viewport_end_offset: placed.viewport,
            total_items_count,
        }
    }

    /// Pins the item with `key`, so it stays composed while out of view, until the
    /// handle returned is released.
    fn pin(&self, key: u64) -> PinnedHandle {
        let first = {
            let mut pinned = self.inner.pinned.borrow_mut();
            let count = pinned.entry(key).or_insert(0);
            *count += 1;
            *count == 1
        };
        if first {
            self.bump();
        }
        let state = self.clone();
        PinnedHandle::new(move || state.unpin(key))
    }

    /// Lets go of one pin of the item with `key`, which leaves the composition once it is
    /// out of view and no other pin holds it.
    fn unpin(&self, key: u64) {
        let last = {
            let mut pinned = self.inner.pinned.borrow_mut();
            match pinned.get_mut(&key) {
                Some(count) if *count > 1 => {
                    *count -= 1;
                    false
                }
                Some(_) => {
                    pinned.remove(&key);
                    true
                }
                None => false,
            }
        };
        if last {
            self.bump();
        }
    }

    /// The container through which the item with `key` is pinned, as the list provides it
    /// in [`LOCAL_PINNABLE_CONTAINER`] to the item: told apart from others by the list and
    /// the key.
    fn pinnable_container(&self, key: u64) -> PinnableContainer {
        let mut hasher = DefaultHasher::new();
        (Rc::as_ptr(&self.inner) as usize, key).hash(&mut hasher);
        let state = self.clone();
        PinnableContainer::new(hasher.finish(), move || state.pin(key))
    }

    /// How far the start of the viewport is past the start of the first item in
    /// view, in logical pixels: Compose's `firstVisibleItemScrollOffset`.
    pub fn first_visible_item_scroll_offset(&self) -> f32 {
        if let Some(uniform) = self.inner.uniform.borrow().as_ref() {
            return uniform.first_visible_item_scroll_offset(self.scroll.offset());
        }
        let placed = self.inner.placed.borrow();
        let count = placed.keys.len();
        if count == 0 {
            return 0.0;
        }
        let offset = self.scroll.offset();
        let first = item_at(&placed.tops, offset).min(count - 1);
        (offset - placed.tops[first]).max(0.0)
    }

    fn bump(&self) {
        self.version.update(|version| *version += 1);
    }

    /// Watches the scroll from now on, if it is not yet watched, so that a scroll taking
    /// the viewport out of what the latest composition covers composes the list again,
    /// and no other scroll does: the composition reads the offset without tracking it.
    fn watch_scroll(&self) {
        if self.inner.watch.borrow().is_some() {
            return;
        }
        let inner = Rc::downgrade(&self.inner);
        let scroll = self.scroll.clone();
        let version = self.version.clone();
        let handle = rustle_runtime::Snapshot::register_apply_observer(move |_| {
            let Some(inner) = inner.upgrade() else {
                return;
            };
            if inner.uncovered.get() {
                return;
            }
            let (start, length) =
                rustle_runtime::untracked(|| (scroll.offset(), scroll.viewport_length()));
            let (from, to) = inner.covered.get();
            let resized = inner
                .composed_viewport
                .get()
                .is_some_and(|composed| composed.to_bits() != length.to_bits());
            if resized || start < from || start + length > to {
                inner.uncovered.set(true);
                version.update(|version| *version += 1);
            }
        });
        *self.inner.watch.borrow_mut() = Some(handle);
    }

    /// Learns that the item with `key` is `height` long, as layout measured it.
    ///
    /// The list composes again when the item was not measured before, or its length
    /// moved by more than half a pixel and could not be taken in during layout: a length
    /// at or past the item at the start of the viewport, such as the newest message of a
    /// chat streaming in, only moves the tops after it and composes nothing.
    fn record(&self, key: u64, height: f32) {
        let old = {
            let mut heights = self.inner.heights.borrow_mut();
            let old = heights.insert(key, height);
            self.inner
                .sum
                .set(self.inner.sum.get() + height - old.unwrap_or(0.0));
            old
        };
        if old.is_some_and(|old| old.to_bits() == height.to_bits()) {
            return;
        }
        let significant = old.is_none_or(|old| (old - height).abs() > 0.5);
        let offset = rustle_runtime::untracked(|| self.scroll.offset());
        let follow = (
            self.inner.stick_to_end.get(),
            self.inner.request.get().is_some(),
        );
        let remeasured = match self.inner.placed.try_borrow_mut() {
            Ok(mut placed) => placed.remeasure(key, height, offset, follow, &self.inner.covered),
            Err(_) => {
                self.inner.rehash.set(true);
                Remeasured::Deferred
            }
        };
        match remeasured {
            Remeasured::Absorbed => {}
            Remeasured::Uncovered => {
                if !self.inner.uncovered.replace(true) {
                    self.bump();
                }
            }
            Remeasured::Unplaced | Remeasured::Deferred => {
                if significant {
                    self.bump();
                }
            }
        }
    }
}

/// Remembers a [`LazyListState`] at this call site.
#[track_caller]
pub fn remember_lazy_list_state() -> LazyListState {
    group(|| LazyListState {
        scroll: remember_scroll_state(),
        inner: remember(|| Rc::new(ListInner::new())).get(),
        version: remember(|| 0),
    })
}

/// How far beyond each end of its viewport a list composes items, as a share of the
/// viewport's length, so that scrolling composes the list again only once it has moved
/// that far rather than on every frame.
const COMPOSED_MARGIN: f32 = 0.25;

/// How close to the end counts as being at it, in pixels.
const END_TOLERANCE: f32 = 2.0;

/// The index of the item containing `offset`, given the item tops, which end
/// with the end of the list.
fn item_at(tops: &[f32], offset: f32) -> usize {
    tops.partition_point(|top| *top <= offset).saturating_sub(1)
}

/// The item a list keeps in place while lengths change above it, for a list whose item
/// `top` is at the start of a viewport ending at `end`: the first item in view whose length
/// was measured, as it was drawn where it is, or `top` when none in view was. An item first
/// measured above it, such as one scrolled into view from above by a guess, then grows
/// upward rather than pushing what was already in view down.
fn steady_anchor(top: usize, end: f32, tops: &[f32], measured: &[bool]) -> usize {
    if measured.get(top).is_none_or(|measured| *measured) {
        return top;
    }
    (top + 1..measured.len())
        .take_while(|&index| tops.get(index).is_some_and(|start| *start < end))
        .find(|&index| measured[index])
        .unwrap_or(top)
}

/// The tops of items with the given `keys`, ending with the end of the list.
/// Items not measured count as `estimate`.
#[cfg(test)]
fn place(keys: &[u64], heights: &KeyMap<f32>, estimate: f32) -> Vec<f32> {
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
    /// Whether each item placed before was measured; empty when all count as measured.
    old_measured: &'a [bool],
    new_keys: &'a [u64],
    new_tops: &'a [f32],
    /// The space after each item but the last, which is part of its measured length.
    spacing: f32,
    /// Whether the list is laid out from its end, its offset counted from there.
    reverse: bool,
}

impl Reposition<'_> {
    /// The offset that keeps the list where the user left it.
    ///
    /// A request is done first. Otherwise, if the list follows its end and was
    /// scrolled to the end, it stays there. A list at its start stays there, so
    /// items inserted at the top show. Otherwise the item that was at the
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
                let top = self.new_tops[index];
                let trailing = if index < last { self.spacing } else { 0.0 };
                let bottom = self.new_tops[index + 1] - trailing;
                let target = match align {
                    ItemAlign::Start if index == 0 => 0.0,
                    ItemAlign::Start => top,
                    ItemAlign::Center => top - (self.viewport - (bottom - top)) / 2.0,
                    ItemAlign::End => bottom - self.viewport,
                };
                return target.clamp(0.0, max);
            }
            Some(Request::At(index, offset)) => {
                let Some(last) = self.new_keys.len().checked_sub(1) else {
                    return 0.0;
                };
                return (self.new_tops[index.min(last)] + offset).clamp(0.0, max);
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
        if self.reverse && self.offset <= END_TOLERANCE {
            return 0.0;
        }
        if self.old_keys.is_empty() || self.offset <= 0.0 {
            return self.offset;
        }
        let top = item_at(self.old_tops, self.offset).min(self.old_keys.len() - 1);
        let anchor = if self.reverse {
            top
        } else {
            steady_anchor(
                top,
                self.offset + self.old_viewport,
                self.old_tops,
                self.old_measured,
            )
        };
        let key = self.old_keys[anchor];
        if self.reverse {
            let short_of_end = self.old_tops[anchor + 1] - self.offset;
            let now = self.new_keys.iter().position(|candidate| *candidate == key);
            return match now {
                Some(index) => self.new_tops[index + 1] - short_of_end,
                None => self.offset,
            };
        }
        let moved_by = self.offset - self.old_tops[anchor];
        let now = if self.new_keys.get(anchor) == Some(&key) {
            Some(anchor)
        } else {
            self.new_keys.iter().position(|candidate| *candidate == key)
        };
        match now {
            Some(index) => self.new_tops[index] + moved_by,
            None => self.offset_by_survivor(anchor),
        }
    }

    /// The offset that keeps in place the first item after `anchor`, the item at the top of
    /// the viewport that is gone, that was in view and is still in the list, as a
    /// placeholder replaced by what it stood for leaves what was below it where it was;
    /// the offset unchanged when none in view is.
    fn offset_by_survivor(&self, anchor: usize) -> f32 {
        let end = self.offset + self.old_viewport.max(self.viewport);
        let wanted: KeyMap<usize> = (anchor + 1..self.old_keys.len())
            .take_while(|&index| self.old_tops[index] < end)
            .map(|index| (self.old_keys[index], index))
            .collect();
        if wanted.is_empty() {
            return self.offset;
        }
        let found = self
            .new_keys
            .iter()
            .enumerate()
            .filter_map(|(now, key)| wanted.get(key).map(|&old| (old, now)))
            .min();
        match found {
            Some((old, now)) => self.new_tops[now] - (self.old_tops[old] - self.offset),
            None => self.offset,
        }
    }
}

fn hash_key(key: impl Hash) -> u64 {
    let mut hasher = KeyHasher::default();
    key.hash(&mut hasher);
    hasher.finish()
}

/// A map by the key of an item, hashed by [`KeyHasher`].
type KeyMap<V> = HashMap<u64, V, std::hash::BuildHasherDefault<KeyHasher>>;

/// A fast hasher for the keys of a list's items and for maps by them: a list hashes the
/// key of every item each time it composes, and looks every item up by it, so with tens of
/// thousands of items the default hasher's cost shows. Not resistant to keys chosen to
/// collide, which a list's keys are not.
#[derive(Default)]
struct KeyHasher(u64);

impl KeyHasher {
    /// Mixes `word` into the hash.
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

impl Hasher for KeyHasher {
    /// The hash of what was written.
    fn finish(&self) -> u64 {
        let mut hash = self.0;
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
        hash ^ (hash >> 33)
    }

    /// Mixes `bytes` in, eight at a time.
    fn write(&mut self, bytes: &[u8]) {
        let (chunks, rest) = bytes.as_chunks::<8>();
        for chunk in chunks {
            self.add(u64::from_le_bytes(*chunk));
        }
        if !rest.is_empty() {
            let mut last = [0; 8];
            last[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(last) ^ ((rest.len() as u64) << 56));
        }
    }

    /// Mixes `value` in as one word.
    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    /// Mixes `value` in as one word.
    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    /// Mixes `value` in as one word.
    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }

    /// Mixes `value` in as one word.
    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }
}

/// Which way a list scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Vertical,
    Horizontal,
}

impl Axis {
    /// The length of `size` along this axis.
    fn length(self, size: Size) -> f32 {
        match self {
            Axis::Vertical => size.height,
            Axis::Horizontal => size.width,
        }
    }

    /// What a list along this axis assumes an unmeasured item to be.
    fn default_estimate(self) -> f32 {
        match self {
            Axis::Vertical => 48.0,
            Axis::Horizontal => 120.0,
        }
    }
}

/// How long items keep sliding after a list reorders, so that moves caused by
/// measuring items are not animated once the slide is over.
const SLIDE_WINDOW: Duration = Duration::from_millis(900);

/// How far an item is from where it is laid out, at `top` along the axis of the list.
///
/// While `sliding`, an item whose `top` changed glides from where it was drawn to its new
/// place as `spec` says; otherwise it is there at once. `held` is how far the list scrolled
/// to keep the view still as items changed, which the item moved with, so it does not glide
/// over that distance.
#[track_caller]
fn slide_shift(top: f32, spec: AnimationSpec, sliding: bool, held: f32) -> f32 {
    group(|| {
        let shown = remember_animatable(top);
        let laid_at = remember(|| Rc::new(Cell::new(top))).get();
        if laid_at.replace(top) != top {
            shown.snap_to(if sliding { shown.value() + held } else { top });
        }
        let glide = shown.clone();
        launched_effect(top.to_bits(), move |scope| async move {
            if sliding && glide.value() != top {
                glide.animate_to(&scope, top, spec).await;
            }
        });
        shown.value() - top
    })
}

/// The space around and between the items of a variable-size lazy list or
/// grid, and whether items slide when the list changes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LazyContent {
    /// Space before the first item, along the scroll axis.
    pub padding_start: f32,
    /// Space after the last item, along the scroll axis.
    pub padding_end: f32,
    /// Space between one item and the next.
    pub spacing: f32,
    /// How items move to their new place when the keys of a list are
    /// reordered, inserted or removed; `None` moves them at once.
    pub animate_item: Option<AnimationSpec>,
    /// Whether the list is laid out from its end: the first item at the bottom
    /// (or the right), the offset counted from there. Compose's `reverseLayout`.
    pub reverse_layout: bool,
}

impl LazyContent {
    /// No padding, no spacing, no animation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the space before the first item and after the last to `padding`.
    pub fn padding(mut self, padding: f32) -> Self {
        self.padding_start = padding;
        self.padding_end = padding;
        self
    }

    /// Sets the space before the first item and the space after the last.
    pub fn padding_each(mut self, start: f32, end: f32) -> Self {
        self.padding_start = start;
        self.padding_end = end;
        self
    }

    /// Sets the space between items.
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Makes items of a keyed list slide to their new place, as `spec` says,
    /// when items are reordered, inserted or removed (the placement animation
    /// of Compose's `Modifier.animateItem`). Items that appear are not faded in
    /// and removed items are gone at once; the others slide.
    pub fn animate_item(mut self, spec: AnimationSpec) -> Self {
        self.animate_item = Some(spec);
        self
    }

    /// Lays the list out from its end, as Compose's `reverseLayout = true` does: the
    /// first item sits at the bottom of a vertical list (the right of a horizontal
    /// one) and the later items above it, the scroll offset counts from there, so
    /// an offset of 0 shows the first item, and the padding at the start is the
    /// padding at that end. Everything else is mirrored with it: the item kept in
    /// place as others are measured or inserted is the one at that end of the
    /// viewport, a list at offset 0 stays there as items grow or are added before
    /// it, and `first_visible_item` is the item nearest that end. A list shorter
    /// than its viewport still starts at the top (or left), as with
    /// `Arrangement.Top`. For a conversation, give the newest message index 0.
    pub fn reverse_layout(mut self, reverse: bool) -> Self {
        self.reverse_layout = reverse;
        self
    }
}

/// Emits `content` in a column for a vertical axis and a row for a horizontal one.
#[track_caller]
fn along(axis: Axis, modifier: Modifier, content: impl FnOnce()) {
    match axis {
        Axis::Vertical => column_with(modifier, content),
        Axis::Horizontal => row_with(modifier, content),
    }
}

/// Emits `len` of empty space along `axis`.
#[track_caller]
fn gap(axis: Axis, len: f32) {
    spacer(match axis {
        Axis::Vertical => Modifier::new().height(len),
        Axis::Horizontal => Modifier::new().width(len),
    });
}

/// Emits the cell of one item: it fills the list across, reports its length
/// along the axis to `state`, is moved `shift` from where it is laid out and is
/// followed by `spacing`.
#[track_caller]
fn item_cell(
    axis: Axis,
    state: &LazyListState,
    item_key: u64,
    spacing: f32,
    shift: f32,
    body: impl FnOnce(),
) {
    item_cell_with(axis, state, item_key, (spacing, false), shift, body);
}

/// Like [`item_cell`], with `spacing` before the item instead of after it when
/// `spacing_first`, as in a list laid out from its end.
#[track_caller]
fn item_cell_with(
    axis: Axis,
    state: &LazyListState,
    item_key: u64,
    (spacing, spacing_first): (f32, bool),
    shift: f32,
    body: impl FnOnce(),
) {
    let learn = state.clone();
    let mut modifier = match axis {
        Axis::Vertical => Modifier::new().fill_max_width(),
        Axis::Horizontal => Modifier::new().fill_max_height(),
    };
    if shift != 0.0 {
        modifier = match axis {
            Axis::Vertical => modifier.offset(0.0, shift),
            Axis::Horizontal => modifier.offset(shift, 0.0),
        };
    }
    let modifier = modifier.on_size_changed(move |size| learn.record(item_key, axis.length(size)));
    along(axis, modifier, || {
        let across = match axis {
            Axis::Vertical => Modifier::new().fill_max_width(),
            Axis::Horizontal => Modifier::new().fill_max_height(),
        };
        if spacing_first && spacing > 0.0 {
            gap(axis, spacing);
        }
        stack_with(across, body);
        if !spacing_first && spacing > 0.0 {
            gap(axis, spacing);
        }
    });
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
    lazy_list_with(modifier, state, LazyContent::new(), count, key_of, item);
}

/// Like [`lazy_list_keyed`], with the padding, spacing and item animation of
/// `content`.
#[track_caller]
pub fn lazy_list_with<K: Hash>(
    modifier: Modifier,
    state: &LazyListState,
    content: LazyContent,
    count: usize,
    key_of: impl Fn(usize) -> K,
    item: impl Fn(usize),
) {
    list_impl(
        Axis::Vertical,
        modifier,
        state,
        content,
        count,
        key_of,
        item,
    );
}

/// A horizontally scrolling [`lazy_list`]: items of any widths, as tall as the list.
#[track_caller]
pub fn lazy_horizontal_list(
    modifier: Modifier,
    state: &LazyListState,
    count: usize,
    item: impl Fn(usize),
) {
    lazy_horizontal_list_keyed(modifier, state, count, |index| index, item);
}

/// A horizontally scrolling [`lazy_list_keyed`]. Widths are learned as items
/// are laid out, and scrolling to an item, following the end and keeping the
/// view while items are measured or inserted before it work as they do
/// vertically.
#[track_caller]
pub fn lazy_horizontal_list_keyed<K: Hash>(
    modifier: Modifier,
    state: &LazyListState,
    count: usize,
    key_of: impl Fn(usize) -> K,
    item: impl Fn(usize),
) {
    lazy_horizontal_list_with(modifier, state, LazyContent::new(), count, key_of, item);
}

/// Like [`lazy_horizontal_list_keyed`], with the padding, spacing and item
/// animation of `content`.
#[track_caller]
pub fn lazy_horizontal_list_with<K: Hash>(
    modifier: Modifier,
    state: &LazyListState,
    content: LazyContent,
    count: usize,
    key_of: impl Fn(usize) -> K,
    item: impl Fn(usize),
) {
    list_impl(
        Axis::Horizontal,
        modifier,
        state,
        content,
        count,
        key_of,
        item,
    );
}

/// The shared body of the variable-size lists, along either axis.
#[track_caller]
fn list_impl<K: Hash>(
    axis: Axis,
    modifier: Modifier,
    state: &LazyListState,
    content: LazyContent,
    count: usize,
    key_of: impl Fn(usize) -> K,
    item: impl Fn(usize),
) {
    group(|| {
        let _ = state.version.get();
        state.watch_scroll();
        state.inner.uncovered.set(false);
        state.inner.uniform.replace(None);
        let measured_viewport = rustle_runtime::untracked(|| state.scroll.viewport_length());
        state.inner.composed_viewport.set(Some(measured_viewport));
        let keys: Vec<u64> = (0..count).map(|index| hash_key(key_of(index))).collect();
        let scrolled = || rustle_runtime::untracked(|| state.scroll.offset());
        let (offset, viewport) = {
            let heights = state.inner.heights.borrow();
            let estimate = if heights.is_empty() {
                axis.default_estimate()
            } else {
                state.inner.sum.get() / heights.len() as f32
            };
            let viewport = match measured_viewport {
                length if length > 0.0 => length,
                _ => 1200.0,
            };
            let mut placed = state.inner.placed.borrow_mut();
            placed.remeasure_in_place = true;
            placed.reverse = content.reverse_layout;
            let old_viewport = placed.viewport;
            let previous = placed.update(
                keys,
                &heights,
                estimate,
                (content.padding_start, content.padding_end),
                state.inner.rehash.replace(false),
            );
            let (old_keys, old_tops, old_measured) = previous.as_ref().map_or(
                (&placed.keys[..], &placed.tops[..], &[][..]),
                |(keys, tops, measured)| (&keys[..], &tops[..], &measured[..]),
            );
            let wanted = Reposition {
                offset: scrolled(),
                viewport,
                old_viewport,
                stick_to_end: state.inner.stick_to_end.get(),
                request: if measured_viewport > 0.0 {
                    state.inner.request.get()
                } else {
                    None
                },
                old_keys,
                old_tops,
                old_measured,
                new_keys: &placed.keys,
                new_tops: &placed.tops,
                spacing: content.spacing,
                reverse: content.reverse_layout,
            }
            .offset();
            if !old_keys.is_empty() && old_keys != placed.keys.as_slice() {
                state
                    .inner
                    .slide_until
                    .set(Some(frame_clock().now() + SLIDE_WINDOW));
            }
            placed.viewport = viewport;
            (wanted, viewport)
        };
        let held = if state.inner.request.get().is_some() {
            0.0
        } else {
            offset - scrolled()
        };
        if (offset - scrolled()).abs() > 0.05 {
            state.scroll.jump_to(offset);
        }
        let (first, last) = {
            let placed = state.inner.placed.borrow();
            let new_tops = &placed.tops;
            let seen = if content.reverse_layout {
                offset.min((new_tops[count] - viewport).max(0.0))
            } else {
                offset
            };
            let margin = viewport * COMPOSED_MARGIN;
            let first = item_at(new_tops, seen - margin).min(count);
            let last = new_tops
                .partition_point(|top| *top < seen + viewport + margin)
                .min(count);
            (first.saturating_sub(1), (last + 1).min(count))
        };
        state.inner.placed.borrow_mut().last = last;
        let placed = state.inner.placed.borrow();
        let (keys, new_tops) = (&placed.keys[..], &placed.tops[..]);
        state.inner.covered.set((
            if first == 0 {
                f32::NEG_INFINITY
            } else {
                new_tops[first]
            },
            if last == count {
                f32::INFINITY
            } else {
                new_tops[last]
            },
        ));
        if let Some(request) = state.inner.request.get() {
            let target = match request {
                Request::Item(index, _) | Request::At(index, _) => index,
                Request::End => usize::MAX,
            };
            let heights = state.inner.heights.borrow();
            let settled = keys
                .get(target.min(keys.len().saturating_sub(1)))
                .is_none_or(|key| heights.contains_key(key))
                && keys[first..last]
                    .iter()
                    .all(|key| heights.contains_key(key));
            if settled && measured_viewport > 0.0 {
                state.inner.request.set(None);
            }
        }
        let sliding = state
            .inner
            .slide_until
            .get()
            .is_some_and(|until| frame_clock().now() < until);
        let scroll_modifier = match axis {
            Axis::Vertical => modifier.vertical_scroll_with(&state.scroll, content.reverse_layout),
            Axis::Horizontal => {
                modifier.horizontal_scroll_with(&state.scroll, content.reverse_layout)
            }
        };

        let composed = composed_items(keys, first, last, &state.inner.pinned.borrow());

        if content.reverse_layout {
            let cells = Cells {
                axis,
                state,
                tops: new_tops,
                spacing: content.spacing,
                padding_end: content.padding_end,
                animate_item: content.animate_item,
                sliding,
                held,
            };
            along(axis, scroll_modifier, || {
                reversed_items(&cells, keys, &composed, &item)
            });
            return;
        }
        along(axis, scroll_modifier, || {
            let mut end = 0.0;
            for &index in &composed {
                let item_key = keys[index];
                gap(axis, new_tops[index] - end);
                end = if index + 1 == count {
                    new_tops[count] - content.padding_end
                } else {
                    new_tops[index + 1]
                };
                key(item_key, || {
                    let shift = match content.animate_item {
                        Some(spec) => slide_shift(new_tops[index], spec, sliding, held),
                        None => 0.0,
                    };
                    let spacing = if index + 1 < count {
                        content.spacing
                    } else {
                        0.0
                    };
                    let container = state.pinnable_container(item_key);
                    item_cell(axis, state, item_key, spacing, shift, || {
                        LOCAL_PINNABLE_CONTAINER.provide(Some(container), || item(index))
                    });
                });
            }
            gap(axis, new_tops[count] - end);
        });
    });
}

/// The items a list composes, in order: those from `first` up to `last`, those outside them
/// that are pinned, and the one on each side of a pinned item, beyond the bounds of what is
/// pinned as the items beyond those in view are, so focus can move on from a pinned item.
fn composed_items(keys: &[u64], first: usize, last: usize, pinned: &KeyMap<usize>) -> Vec<usize> {
    if pinned.is_empty() {
        return (first..last).collect();
    }
    let near_pinned = |index: usize| {
        (index.saturating_sub(1)..(index + 2).min(keys.len()))
            .any(|near| pinned.contains_key(&keys[near]))
    };
    let before = (0..first).filter(|index| near_pinned(*index));
    let after = (last..keys.len()).filter(|index| near_pinned(*index));
    before.chain(first..last).chain(after).collect()
}

/// Where the items of a list composing go and how they move: what
/// [`reversed_items`] needs besides the items themselves.
struct Cells<'a> {
    /// Which way the list scrolls.
    axis: Axis,
    /// The state the items report their lengths to.
    state: &'a LazyListState,
    /// The near edge of each item counted from the start of the list, and, last, the
    /// length of the list.
    tops: &'a [f32],
    /// The space between one item and the next.
    spacing: f32,
    /// The space after the last item, which the length of the list counts.
    padding_end: f32,
    /// How items slide to new places, if they do.
    animate_item: Option<AnimationSpec>,
    /// Whether items are sliding now.
    sliding: bool,
    /// How far the list scrolled to keep the view still.
    held: f32,
}

/// Emits the items `composed` (ascending indexes into `keys`) of a list laid out from
/// its end: the last of them first, each after the space between it and the one
/// drawn before, then the space from the first item to the end of the list.
#[track_caller]
fn reversed_items(cells: &Cells<'_>, keys: &[u64], composed: &[usize], item: &impl Fn(usize)) {
    let count = keys.len();
    let mut drawn_to = cells.tops[count];
    for &index in composed.iter().rev() {
        let item_key = keys[index];
        let end = if index + 1 == count {
            cells.tops[count] - cells.padding_end
        } else {
            cells.tops[index + 1]
        };
        gap(cells.axis, drawn_to - end);
        drawn_to = cells.tops[index];
        key(item_key, || {
            let shift = match cells.animate_item {
                Some(spec) => -slide_shift(cells.tops[index], spec, cells.sliding, cells.held),
                None => 0.0,
            };
            let spacing = if index + 1 < count {
                cells.spacing
            } else {
                0.0
            };
            let container = cells.state.pinnable_container(item_key);
            item_cell_with(
                cells.axis,
                cells.state,
                item_key,
                (spacing, true),
                shift,
                || LOCAL_PINNABLE_CONTAINER.provide(Some(container), || item(index)),
            );
        });
    }
    gap(cells.axis, drawn_to);
}

/// Which header a list shows pinned at its top, and how far the next header
/// pushes it up: the last header at or before the first item in view, moved up
/// by what the next header has overlapped of it.
fn sticky_header(
    tops: &[f32],
    offset: f32,
    count: usize,
    is_header: impl Fn(usize) -> bool,
    header_length: impl Fn(usize) -> f32,
) -> Option<(usize, f32)> {
    if count == 0 {
        return None;
    }
    let first = item_at(tops, offset).min(count - 1);
    let current = (0..=first).rev().find(|&index| is_header(index))?;
    let shift = (current + 1..count)
        .find(|&index| is_header(index))
        .map_or(0.0, |next| {
            (tops[next] - offset - header_length(current)).min(0.0)
        });
    Some((current, shift))
}

/// Like [`lazy_list_with`], with headers that stay at the top of the list
/// while the items of their section scroll under them, until the next header
/// pushes them away. `is_header` says which items are headers; the pinned one
/// is drawn again over the list at its top, so it should have an opaque
/// background.
#[track_caller]
pub fn lazy_sticky_list<K: Hash>(
    modifier: Modifier,
    state: &LazyListState,
    content: LazyContent,
    count: usize,
    key_of: impl Fn(usize) -> K,
    is_header: impl Fn(usize) -> bool,
    item: impl Fn(usize),
) {
    sticky_impl(
        Axis::Vertical,
        modifier,
        state,
        content,
        count,
        key_of,
        is_header,
        item,
    );
}

/// [`lazy_sticky_list`] turned sideways: a horizontally scrolling list whose
/// headers stay at its start edge (its left; it is not mirrored in a
/// right-to-left layout) while the items of their section scroll under
/// them, until the next header pushes them away. The pinned header is drawn
/// again over the list, so it should have an opaque background.
#[track_caller]
pub fn lazy_horizontal_sticky_list<K: Hash>(
    modifier: Modifier,
    state: &LazyListState,
    content: LazyContent,
    count: usize,
    key_of: impl Fn(usize) -> K,
    is_header: impl Fn(usize) -> bool,
    item: impl Fn(usize),
) {
    sticky_impl(
        Axis::Horizontal,
        modifier,
        state,
        content,
        count,
        key_of,
        is_header,
        item,
    );
}

/// The shared body of the sticky lists, along either axis.
#[allow(clippy::too_many_arguments)]
#[track_caller]
fn sticky_impl<K: Hash>(
    axis: Axis,
    modifier: Modifier,
    state: &LazyListState,
    content: LazyContent,
    count: usize,
    key_of: impl Fn(usize) -> K,
    is_header: impl Fn(usize) -> bool,
    item: impl Fn(usize),
) {
    group(|| {
        stack_with(modifier, || {
            list_impl(
                axis,
                Modifier::new().fill_max_size(),
                state,
                content,
                count,
                &key_of,
                &item,
            );
            state.inner.placed.borrow_mut().remeasure_in_place = false;
            let pinned = {
                let placed = state.inner.placed.borrow();
                let heights = state.inner.heights.borrow();
                sticky_header(
                    &placed.tops,
                    state.scroll.offset(),
                    count,
                    &is_header,
                    |index| {
                        let spacing = if index + 1 < count {
                            content.spacing
                        } else {
                            0.0
                        };
                        placed
                            .keys
                            .get(index)
                            .and_then(|key| heights.get(key))
                            .map_or(0.0, |length| (length - spacing).max(0.0))
                    },
                )
            };
            if let Some((index, shift)) = pinned {
                key(("sticky", hash_key(key_of(index))), || {
                    let pinned = match axis {
                        Axis::Vertical => Modifier::new().fill_max_width().offset(0.0, shift),
                        Axis::Horizontal => Modifier::new().fill_max_height().offset(shift, 0.0),
                    };
                    stack_with(pinned, || item(index));
                });
            }
        });
    });
}

/// The result of dealing items into the lanes of a staggered grid.
#[derive(Debug, Default, PartialEq)]
struct Lanes {
    /// The indices of the items of each lane, in order.
    items: Vec<Vec<usize>>,
    /// The top of each item of each lane and, last, where the lane's items end.
    tops: Vec<Vec<f32>>,
    /// The end of each lane, with its end padding.
    ends: Vec<f32>,
}

/// Deals items into `lanes` lanes: an item not dealt before goes to the
/// shortest lane at that point and stays in its lane after, so items do not
/// jump as others are measured. Items not measured count as `estimate`, each
/// followed by `spacing`.
fn assign_lanes(
    keys: &[u64],
    assigned: &mut KeyMap<usize>,
    heights: &KeyMap<f32>,
    estimate: f32,
    lanes: usize,
    content: &LazyContent,
) -> Lanes {
    let mut out = Lanes {
        items: vec![Vec::new(); lanes],
        tops: vec![Vec::new(); lanes],
        ends: vec![0.0; lanes],
    };
    let mut lengths = vec![content.padding_start; lanes];
    for (index, key) in keys.iter().enumerate() {
        let lane = assigned
            .get(key)
            .copied()
            .filter(|lane| *lane < lanes)
            .unwrap_or_else(|| {
                let lane = lengths
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.total_cmp(b.1))
                    .map_or(0, |(lane, _)| lane);
                assigned.insert(*key, lane);
                lane
            });
        out.items[lane].push(index);
        out.tops[lane].push(lengths[lane]);
        lengths[lane] += heights.get(key).copied().unwrap_or(estimate);
    }
    for (lane, length) in lengths.iter().enumerate() {
        out.tops[lane].push(*length);
        out.ends[lane] = length + content.padding_end;
    }
    out
}

/// The offset that keeps a staggered grid where the user left it: the item that was
/// at the start of the viewport (the first one, lane by lane, that contained `offset`)
/// stays exactly where it was, however the items above it in its lane were measured.
/// `previous` is where each item was, by key, `dealt` and `keys` say where they are now.
fn held_offset(offset: f32, previous: &KeyMap<f32>, dealt: &Lanes, keys: &[u64]) -> f32 {
    if previous.is_empty() || offset <= 0.0 {
        return offset;
    }
    for lane in 0..dealt.items.len() {
        for (slot, &index) in dealt.items[lane].iter().enumerate() {
            let Some(&before) = previous.get(&keys[index]) else {
                continue;
            };
            let after = dealt.tops[lane][slot];
            let length = dealt.tops[lane][slot + 1] - after;
            let next_before = dealt.items[lane]
                .get(slot + 1)
                .and_then(|&next| previous.get(&keys[next]).copied())
                .unwrap_or(before + length);
            if before <= offset && offset < next_before {
                return offset + (after - before);
            }
        }
    }
    offset
}

/// The shared body of the staggered grids, along either axis.
#[allow(clippy::too_many_arguments)]
#[track_caller]
fn staggered_impl<K: Hash>(
    axis: Axis,
    modifier: Modifier,
    state: &LazyListState,
    content: LazyContent,
    lanes: usize,
    count: usize,
    key_of: impl Fn(usize) -> K,
    item: impl Fn(usize),
) {
    group(|| {
        let _ = state.version.get();
        let lanes = lanes.max(1);
        let keys: Vec<u64> = (0..count).map(|index| hash_key(key_of(index))).collect();
        let dealt = {
            let heights = state.inner.heights.borrow();
            let estimate = if heights.is_empty() {
                axis.default_estimate()
            } else {
                state.inner.sum.get() / heights.len() as f32
            };
            let mut assigned = state.inner.lanes.borrow_mut();
            if assigned.0 != lanes {
                *assigned = (lanes, KeyMap::default());
            }
            assign_lanes(&keys, &mut assigned.1, &heights, estimate, lanes, &content)
        };
        let viewport = match state.scroll.viewport_length() {
            length if length > 0.0 => length,
            _ => 1200.0,
        };
        let offset = {
            let mut previous = state.inner.stagger.borrow_mut();
            let wanted = held_offset(state.scroll.offset(), &previous.1, &dealt, &keys);
            if !previous.0.is_empty() && previous.0 != keys {
                state
                    .inner
                    .slide_until
                    .set(Some(frame_clock().now() + SLIDE_WINDOW));
            }
            previous.0.clone_from(&keys);
            previous.1.clear();
            for lane in 0..lanes {
                for (slot, &index) in dealt.items[lane].iter().enumerate() {
                    previous.1.insert(keys[index], dealt.tops[lane][slot]);
                }
            }
            wanted
        };
        let held = offset - state.scroll.offset();
        if held.abs() > 0.05 {
            state.scroll.jump_to(offset);
        }
        let sliding = state
            .inner
            .slide_until
            .get()
            .is_some_and(|until| frame_clock().now() < until);
        let scroll_modifier = match axis {
            Axis::Vertical => modifier.vertical_scroll(&state.scroll),
            Axis::Horizontal => modifier.horizontal_scroll(&state.scroll),
        };
        let across = match axis {
            Axis::Vertical => Axis::Horizontal,
            Axis::Horizontal => Axis::Vertical,
        };
        along(across, scroll_modifier, || {
            for lane in 0..lanes {
                let tops = &dealt.tops[lane];
                let indices = &dealt.items[lane];
                let n = indices.len();
                let first = item_at(tops, offset).min(n).saturating_sub(1);
                let last = (tops.partition_point(|top| *top < offset + viewport).min(n) + 1).min(n);
                key(lane, || {
                    along(axis, Modifier::new().weight(1.0), || {
                        gap(axis, tops[first]);
                        for (slot, &index) in indices.iter().enumerate().take(last).skip(first) {
                            let item_key = keys[index];
                            key(item_key, || {
                                let shift = match content.animate_item {
                                    Some(spec) => slide_shift(tops[slot], spec, sliding, held),
                                    None => 0.0,
                                };
                                item_cell(axis, state, item_key, content.spacing, shift, || {
                                    item(index)
                                });
                            });
                        }
                        gap(axis, dealt.ends[lane] - tops[last]);
                    });
                });
            }
        });
    });
}

/// Emits a vertically scrolling grid of `lanes` columns of equal width whose
/// items have any heights, each placed under the shortest column when it is
/// first composed (a masonry layout), composing only the items near the viewport.
///
/// Heights are learned as items are laid out, as in [`lazy_list`], and an
/// item keeps its column once placed. The view is held the way [`lazy_list`] does: the
/// item at the top of the first column in view stays where it is while items
/// above it are measured. `content` gives the padding
/// and the spacing after each item; its `animate_item` slides the items that
/// stay when the keys are reordered, inserted or removed.
#[track_caller]
pub fn lazy_vertical_staggered_grid<K: Hash>(
    modifier: Modifier,
    state: &LazyListState,
    content: LazyContent,
    lanes: usize,
    count: usize,
    key_of: impl Fn(usize) -> K,
    item: impl Fn(usize),
) {
    staggered_impl(
        Axis::Vertical,
        modifier,
        state,
        content,
        lanes,
        count,
        key_of,
        item,
    );
}

/// [`lazy_vertical_staggered_grid`] turned sideways: `lanes` rows of equal
/// height whose items have any widths, scrolling horizontally.
#[track_caller]
pub fn lazy_horizontal_staggered_grid<K: Hash>(
    modifier: Modifier,
    state: &LazyListState,
    content: LazyContent,
    lanes: usize,
    count: usize,
    key_of: impl Fn(usize) -> K,
    item: impl Fn(usize),
) {
    staggered_impl(
        Axis::Horizontal,
        modifier,
        state,
        content,
        lanes,
        count,
        key_of,
        item,
    );
}

/// Packs items into rows of `columns` columns, each item taking `span_of(index)`
/// of them (at least one, at most `columns`), starting a row when the next one
/// does not fit.
fn pack_rows(count: usize, columns: usize, span_of: impl Fn(usize) -> usize) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut used = 0;
    for index in 0..count {
        let span = span_of(index).clamp(1, columns);
        if rows.is_empty() || used + span > columns {
            rows.push(Vec::new());
            used = 0;
        }
        if let Some(row) = rows.last_mut() {
            row.push(index);
        }
        used += span;
    }
    rows
}

/// Emits a vertically scrolling grid of `columns` columns whose items span
/// `span_of(index)` columns and have any heights: items fill a row in order,
/// the next row starts when one does not fit, and a row is as tall as its
/// tallest item. Only the rows near the viewport are composed.
///
/// Rows are the items of a [`lazy_list_with`], so their heights are learned
/// and the view is kept as they are measured; `content` applies between rows.
/// [`LazyListState::scroll_to_item`] takes the index of an item.
#[allow(clippy::too_many_arguments)]
#[track_caller]
pub fn lazy_vertical_grid_spanned<K: Hash>(
    modifier: Modifier,
    state: &LazyListState,
    content: LazyContent,
    columns: usize,
    count: usize,
    key_of: impl Fn(usize) -> K,
    span_of: impl Fn(usize) -> usize,
    item: impl Fn(usize),
) {
    let columns = columns.max(1);
    let rows = pack_rows(count, columns, &span_of);
    state.inner.grid_rows.borrow_mut().clone_from(&rows);
    lazy_list_with(
        modifier,
        state,
        content,
        rows.len(),
        |row| key_of(rows[row][0]),
        |row| {
            row_with(Modifier::new().fill_max_width(), || {
                let mut used = 0;
                for &index in &rows[row] {
                    let span = span_of(index).clamp(1, columns);
                    used += span;
                    key(index, || {
                        stack_with(Modifier::new().weight(span as f32), || item(index));
                    });
                }
                if used < columns {
                    spacer(Modifier::new().weight((columns - used) as f32));
                }
            });
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heights(pairs: &[(u64, f32)]) -> KeyMap<f32> {
        pairs.iter().copied().collect()
    }

    /// The Fenwick tree sums, finds and updates lengths as a running sum over them does.
    #[test]
    fn lengths_sum_find_and_change_items() {
        let mut lengths = Lengths::new(vec![10.0, 20.0, 0.0, 30.0, 5.0]);
        let sums = [0.0, 10.0, 30.0, 30.0, 60.0, 65.0];
        for (end, sum) in sums.iter().enumerate() {
            assert_eq!(lengths.prefix(end), *sum);
        }
        assert_eq!(lengths.index_at(-1.0), 0);
        assert_eq!(lengths.index_at(9.9), 0);
        assert_eq!(lengths.index_at(10.0), 1);
        assert_eq!(lengths.index_at(30.0), 3);
        assert_eq!(lengths.index_at(64.0), 4);
        assert_eq!(lengths.index_at(100.0), 5);
        lengths.set(1, 50.0);
        assert_eq!(lengths.get(1), 50.0);
        assert_eq!(lengths.prefix(2), 60.0);
        assert_eq!(lengths.prefix(5), 95.0);
        assert_eq!(lengths.index_at(60.0), 3);
    }

    #[test]
    fn items_are_placed_by_measured_height_or_the_estimate() {
        let tops = place(&[1, 2, 3], &heights(&[(1, 10.0), (3, 30.0)]), 20.0);
        assert_eq!(tops, vec![0.0, 10.0, 30.0, 60.0]);
        assert_eq!(item_at(&tops, 0.0), 0);
        assert_eq!(item_at(&tops, 29.9), 1);
        assert_eq!(item_at(&tops, 30.0), 2);
    }

    #[test]
    fn a_staggered_grid_holds_the_item_in_view_when_items_above_are_measured() {
        let keys = [1, 2, 3];
        let previous: KeyMap<f32> = [(1, 0.0), (2, 50.0), (3, 0.0)].into_iter().collect();
        let dealt = Lanes {
            items: vec![vec![0, 1], vec![2]],
            tops: vec![vec![0.0, 80.0, 130.0], vec![0.0, 40.0]],
            ends: vec![130.0, 40.0],
        };
        assert_eq!(held_offset(60.0, &previous, &dealt, &keys), 90.0);
        assert_eq!(held_offset(0.0, &previous, &dealt, &keys), 0.0);
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
            old_measured: &[],
            new_keys: new.0,
            new_tops: new.1,
            spacing: 0.0,
            reverse: false,
        }
    }

    /// An item scrolled into view from above by a guess that turns out longer grows upward,
    /// keeping the first item in view that was measured where it was drawn; and a guessed
    /// item at the top of the view that is replaced keeps the next item in view in place.
    #[test]
    fn items_guessed_above_what_was_drawn_grow_upward() {
        let keys = [1, 2, 3];
        let old = [0.0, 50.0, 100.0, 150.0];
        let grown = [0.0, 50.0, 250.0, 300.0];
        let mut list = reposition(80.0, (&keys, &old), (&keys, &grown));
        let measured = [true, false, true];
        list.old_measured = &measured;
        assert_eq!(list.offset(), 230.0);
        let replaced = [1, 4, 5, 3];
        let new = [0.0, 50.0, 150.0, 300.0, 350.0];
        let list = reposition(80.0, (&keys, &old), (&replaced, &new));
        assert_eq!(list.offset(), 280.0);
    }

    /// A list laid out from its end and scrolled away from it keeps what is in view where
    /// it was when the item in view grows at its near side, as a streamed reply does, and
    /// when the newest item below the view grows; scrolled back to within a few pixels of
    /// its end, it follows the end again.
    #[test]
    fn a_reversed_list_scrolled_away_keeps_its_view_as_items_grow() {
        let keys = [1, 2, 3];
        let old = [0.0, 60.0, 120.0, 180.0];
        let grown_in_view = [0.0, 60.0, 150.0, 210.0];
        let mut list = reposition(90.0, (&keys, &old), (&keys, &grown_in_view));
        list.reverse = true;
        assert_eq!(list.offset(), 120.0);
        let grown_below = [0.0, 100.0, 160.0, 220.0];
        let mut list = reposition(90.0, (&keys, &old), (&keys, &grown_below));
        list.reverse = true;
        assert_eq!(list.offset(), 130.0);
        let mut near_end = reposition(1.5, (&keys, &old), (&keys, &grown_below));
        near_end.reverse = true;
        assert_eq!(near_end.offset(), 0.0);
    }

    /// A length learned for the item at the start of the view of a reversed list scrolled
    /// away from its end waits for the next composition rather than moving what is in view,
    /// while one for an item past it is taken in at once.
    #[test]
    fn a_reversed_list_scrolled_away_defers_growth_at_its_view() {
        let mut placed = Placed {
            remeasure_in_place: true,
            reverse: true,
            viewport: 100.0,
            ..Placed::default()
        };
        let heights: KeyMap<f32> = [1, 2, 3, 4].into_iter().map(|key| (key, 60.0)).collect();
        placed.update(vec![1, 2, 3, 4], &heights, 60.0, (0.0, 0.0), false);
        placed.last = 4;
        let covered = Cell::new((0.0, f32::INFINITY));
        assert_eq!(
            placed.remeasure(2, 90.0, 70.0, (false, false), &covered),
            Remeasured::Deferred
        );
        assert_eq!(
            placed.remeasure(4, 90.0, 70.0, (false, false), &covered),
            Remeasured::Absorbed
        );
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

    #[test]
    fn staggered_items_go_to_the_shortest_lane_and_stay() {
        let keys = [1, 2, 3, 4];
        let mut assigned = KeyMap::default();
        let known = heights(&[(1, 100.0), (2, 30.0), (3, 30.0)]);
        let content = LazyContent::new().padding_each(10.0, 5.0);
        let lanes = assign_lanes(&keys, &mut assigned, &known, 20.0, 2, &content);
        assert_eq!(lanes.items, vec![vec![0], vec![1, 2, 3]]);
        assert_eq!(lanes.tops[1], vec![10.0, 40.0, 70.0, 90.0]);
        assert_eq!(lanes.ends, vec![115.0, 95.0]);
        let again = assign_lanes(
            &keys,
            &mut assigned,
            &heights(&[(1, 10.0)]),
            20.0,
            2,
            &content,
        );
        assert_eq!(again.items, lanes.items);
    }

    #[test]
    fn items_are_packed_into_rows_by_span() {
        let rows = pack_rows(5, 3, |index| if index == 1 { 2 } else { 1 });
        assert_eq!(rows, vec![vec![0, 1], vec![2, 3, 4]]);
    }

    #[test]
    fn the_sticky_header_is_pushed_up_by_the_next() {
        let tops = [0.0, 40.0, 80.0, 120.0, 160.0];
        let is_header = |index: usize| index.is_multiple_of(3);
        assert_eq!(
            sticky_header(&tops, 50.0, 4, is_header, |_| 30.0),
            Some((0, 0.0))
        );
        assert_eq!(
            sticky_header(&tops, 105.0, 4, is_header, |_| 30.0),
            Some((0, -15.0))
        );
        assert_eq!(
            sticky_header(&tops, 125.0, 4, is_header, |_| 30.0),
            Some((3, 0.0))
        );
        assert_eq!(sticky_header(&tops, 0.0, 0, is_header, |_| 30.0), None);
    }
}
