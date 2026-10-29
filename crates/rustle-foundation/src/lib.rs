//! Design-system-agnostic building blocks: layouts and basic elements.
//!
//! Everything here is a composable built on the primitives of `rustle-ui`.
//! Each element has a plain form and a `_with` form taking a [`Modifier`].

mod text_field;

use rustle_geometry::{Offset, Size};
use rustle_graphics::ImageData;
use rustle_runtime::{CompositionLocal, group, key};
use rustle_text::{TextSpan, TextStyle};
use rustle_ui::{Children, Content, Layout, LayoutResult, Node, Overlay, emit, layout};
use rustle_unit::Constraints;

pub use rustle_ui::Modifier;
pub use rustle_ui::{
    BoxAlignment, Canvas, ContentScale, Focus, Key, KeyEvent, LOCAL_LAYOUT_DIRECTION,
    LayoutDirection, Modifiers, Role, ScrollState, Semantics, canvas, is_rtl, remember_focus,
    remember_scroll_state,
};
pub use text_field::{
    TextFieldOptions, TextFieldState, TextFieldValue, basic_text_area, basic_text_field,
    basic_text_field_in, remember_text_field_state, selectable_text,
};

/// The style `text` is set in. Provide another value to restyle everything below.
pub static LOCAL_TEXT_STYLE: CompositionLocal<TextStyle> =
    CompositionLocal::new(TextStyle::default);

/// Whether the system is set to a dark appearance. Provided by the host and
/// kept up to date as the setting changes.
pub static LOCAL_SYSTEM_DARK: CompositionLocal<bool> = CompositionLocal::new(|| true);

/// Whether the system is set to a dark appearance.
pub fn is_system_in_dark_theme() -> bool {
    LOCAL_SYSTEM_DARK.current()
}

/// Emits a line of text in the current [`LOCAL_TEXT_STYLE`].
pub fn text(content: impl Into<String>) {
    text_with(Modifier::new(), content);
}

/// Emits a line of text with `modifier`, in the current [`LOCAL_TEXT_STYLE`].
pub fn text_with(modifier: Modifier, content: impl Into<String>) {
    text_styled(modifier, content, LOCAL_TEXT_STYLE.current());
}

/// Emits a line of text with `modifier`, set in `style`.
pub fn text_styled(modifier: Modifier, content: impl Into<String>, style: TextStyle) {
    emit(Node {
        modifier,
        content: Content::Text {
            content: content.into(),
            style,
            editing: None,
        },
    });
}

/// Emits a picture, sized to its pixels unless `modifier` or the space available says otherwise.
/// `scale` says how it fills an area of a different shape.
#[track_caller]
pub fn image(modifier: Modifier, image: &ImageData, scale: ContentScale) {
    emit(Node {
        modifier,
        content: Content::Image {
            image: image.clone(),
            scale,
        },
    });
}

/// Emits a line of text made of runs in their own colors, weights or fonts,
/// set in the current [`LOCAL_TEXT_STYLE`].
#[track_caller]
pub fn rich_text(spans: Vec<TextSpan>) {
    rich_text_with(Modifier::new(), spans);
}

/// Emits rich text with `modifier`.
#[track_caller]
pub fn rich_text_with(modifier: Modifier, spans: Vec<TextSpan>) {
    emit(Node {
        modifier,
        content: Content::RichText {
            spans,
            style: LOCAL_TEXT_STYLE.current(),
        },
    });
}

/// Emits a column that places the children of `content` top to bottom.
#[track_caller]
pub fn column(content: impl FnOnce()) {
    column_with(Modifier::new(), content);
}

/// Emits a column with `modifier`.
#[track_caller]
pub fn column_with(modifier: Modifier, content: impl FnOnce()) {
    layout(
        modifier,
        Linear::new(true, Arrangement::Start, Alignment::Start),
        content,
    );
}

/// Emits a row that places the children of `content` left to right.
#[track_caller]
pub fn row(content: impl FnOnce()) {
    row_with(Modifier::new(), content);
}

/// Emits a row with `modifier`.
#[track_caller]
pub fn row_with(modifier: Modifier, content: impl FnOnce()) {
    layout(
        modifier,
        Linear::new(false, Arrangement::Start, Alignment::Start),
        content,
    );
}

/// Emits a stack that places the children of `content` on top of each other.
///
/// This is Compose's `Box`, which Rust reserves as a keyword.
#[track_caller]
pub fn stack(content: impl FnOnce()) {
    stack_with(Modifier::new(), content);
}

/// Emits a stack with `modifier`, putting children without their own `align` at `alignment`.
#[track_caller]
pub fn stack_aligned(modifier: Modifier, alignment: BoxAlignment, content: impl FnOnce()) {
    layout(
        modifier,
        Overlay {
            alignment,
            rtl: is_rtl(),
        },
        content,
    );
}

/// Emits a stack with `modifier`.
#[track_caller]
pub fn stack_with(modifier: Modifier, content: impl FnOnce()) {
    layout(modifier, Overlay::default(), content);
}

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

/// Emits a row that fills with the children of `content` left to right and
/// starts a new line when the next one does not fit, with the given gaps.
#[track_caller]
pub fn flow_row(
    modifier: Modifier,
    horizontal_spacing: f32,
    vertical_spacing: f32,
    content: impl FnOnce(),
) {
    layout(
        modifier,
        Flow {
            horizontal: horizontal_spacing,
            vertical: vertical_spacing,
            rtl: is_rtl(),
        },
        content,
    );
}

/// Places children in lines, wrapping when the next does not fit the width.
struct Flow {
    horizontal: f32,
    vertical: f32,
    rtl: bool,
}

impl Layout for Flow {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let child_constraints = Constraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: constraints.max_width,
            max_height: f32::INFINITY,
        };
        let (mut x, mut y, mut line_height, mut width) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        let mut positions = Vec::with_capacity(children.len());
        let mut sizes = Vec::with_capacity(children.len());
        for index in 0..children.len() {
            let size = children.measure(index, child_constraints);
            sizes.push(size);
            if x > 0.0 && x + size.width > constraints.max_width {
                x = 0.0;
                y += line_height + self.vertical;
                line_height = 0.0;
            }
            positions.push(Offset::new(x, y));
            x += size.width + self.horizontal;
            line_height = line_height.max(size.height);
            width = width.max(x - self.horizontal);
        }
        if self.rtl {
            for (position, size) in positions.iter_mut().zip(&sizes) {
                position.x = width - position.x - size.width;
            }
        }
        LayoutResult {
            size: Size::new(width, y + line_height),
            positions,
        }
    }
}

/// Emits `content` showing only `fraction` (0 to 1) of its height, cut off
/// below. For collapsing and expanding things.
#[track_caller]
pub fn reveal(fraction: f32, modifier: Modifier, content: impl FnOnce()) {
    layout(
        modifier.clip(),
        Reveal {
            fraction: fraction.clamp(0.0, 1.0),
        },
        content,
    );
}

/// Measures children at their natural height and takes a fraction of it.
struct Reveal {
    fraction: f32,
}

impl Layout for Reveal {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let natural = Constraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: constraints.max_width,
            max_height: f32::INFINITY,
        };
        let mut size = Size::default();
        for index in 0..children.len() {
            let child = children.measure(index, natural);
            size.width = size.width.max(child.width);
            size.height = size.height.max(child.height);
        }
        LayoutResult {
            size: Size::new(size.width, size.height * self.fraction),
            positions: vec![Offset::default(); children.len()],
        }
    }
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

/// Emits `content` in a layer above everything else, as large as it likes,
/// taking no space where it is emitted: its top-left corner is where it would
/// have started. For popups, menus and tooltips. Position it with `align`.
#[track_caller]
pub fn floating(modifier: Modifier, content: impl FnOnce()) {
    layout(modifier.z_index(1), Floating, content);
}

/// Measures children without limits and takes no space itself.
struct Floating;

impl Layout for Floating {
    fn measure(&self, children: &mut Children, _: Constraints) -> LayoutResult {
        let unbounded = Constraints {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
        };
        for index in 0..children.len() {
            children.measure(index, unbounded);
        }
        LayoutResult {
            size: Size::default(),
            positions: vec![Offset::default(); children.len()],
        }
    }
}

/// Emits empty space sized by `modifier`, such as `Modifier::new().height(8.0)`.
#[track_caller]
pub fn spacer(modifier: Modifier) {
    stack_with(modifier, || {});
}

/// How a row or column spreads its children along its main axis when it has room to spare.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arrangement {
    Start,
    Center,
    End,
    /// The first child at the start, the last at the end, equal gaps between.
    SpaceBetween,
    /// Equal gaps between children, half of one at both ends.
    SpaceAround,
    /// Equal gaps between children and at both ends.
    SpaceEvenly,
    /// A fixed gap between children, starting at the start.
    SpacedBy(f32),
}

/// Where a row or column puts each child across its main axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Alignment {
    Start,
    Center,
    End,
}

/// Emits a column with `arrangement` down the column and `alignment` across it.
#[track_caller]
pub fn column_arranged(
    modifier: Modifier,
    arrangement: Arrangement,
    alignment: Alignment,
    content: impl FnOnce(),
) {
    layout(modifier, Linear::new(true, arrangement, alignment), content);
}

/// Emits a row with `arrangement` along the row and `alignment` across it.
#[track_caller]
pub fn row_arranged(
    modifier: Modifier,
    arrangement: Arrangement,
    alignment: Alignment,
    content: impl FnOnce(),
) {
    layout(
        modifier,
        Linear::new(false, arrangement, alignment),
        content,
    );
}

/// A column or a row.
struct Linear {
    vertical: bool,
    arrangement: Arrangement,
    alignment: Alignment,
    /// Mirrors rows, and which side a column's start alignment is.
    rtl: bool,
}

impl Linear {
    fn new(vertical: bool, arrangement: Arrangement, alignment: Alignment) -> Self {
        Self {
            vertical,
            arrangement,
            alignment,
            rtl: is_rtl(),
        }
    }

    /// Splits a size into (main, cross) lengths.
    fn split(&self, size: Size) -> (f32, f32) {
        if self.vertical {
            (size.height, size.width)
        } else {
            (size.width, size.height)
        }
    }

    fn join(&self, main: f32, cross: f32) -> Size {
        if self.vertical {
            Size::new(cross, main)
        } else {
            Size::new(main, cross)
        }
    }

    fn constraints(&self, main: (f32, f32), cross_max: f32) -> Constraints {
        let (width, height) = if self.vertical {
            ((0.0, cross_max), main)
        } else {
            (main, (0.0, cross_max))
        };
        Constraints {
            min_width: width.0,
            max_width: width.1,
            min_height: height.0,
            max_height: height.1,
        }
    }
}

impl Layout for Linear {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let (main_min, cross_min) =
            self.split(Size::new(constraints.min_width, constraints.min_height));
        let (main_max, cross_max) =
            self.split(Size::new(constraints.max_width, constraints.max_height));
        let count = children.len();
        let spacing = match self.arrangement {
            Arrangement::SpacedBy(spacing) => spacing,
            _ => 0.0,
        };
        let mut mains = vec![0.0_f32; count];
        let mut crosses = vec![0.0_f32; count];
        let mut used = spacing * count.saturating_sub(1) as f32;

        // Children without a weight take what they ask for, in order.
        let mut total_weight = 0.0;
        for index in 0..count {
            if let Some(weight) = children.weight(index) {
                total_weight += weight.max(0.0);
                continue;
            }
            let room = (main_max - used).max(0.0);
            let size = children.measure(index, self.constraints((0.0, room), cross_max));
            (mains[index], crosses[index]) = self.split(size);
            used += mains[index];
        }

        // Weighted children split what is left, when there is a bound to leave it.
        if total_weight > 0.0 {
            let space = if main_max.is_finite() {
                (main_max - used).max(0.0)
            } else {
                0.0
            };
            for index in 0..count {
                let Some(weight) = children.weight(index) else {
                    continue;
                };
                let share = space * weight.max(0.0) / total_weight;
                let size = children.measure(index, self.constraints((share, share), cross_max));
                (mains[index], crosses[index]) = self.split(size);
                used += mains[index];
            }
        }

        let main_size = used.max(main_min).min(main_max);
        let cross_size = crosses
            .iter()
            .fold(cross_min, |widest, cross| widest.max(*cross))
            .min(cross_max.max(cross_min));

        let free = (main_size - used).max(0.0);
        let (start, gap) = match self.arrangement {
            Arrangement::Start => (0.0, 0.0),
            Arrangement::Center => (free / 2.0, 0.0),
            Arrangement::End => (free, 0.0),
            Arrangement::SpaceBetween if count > 1 => (0.0, free / (count - 1) as f32),
            Arrangement::SpaceBetween => (0.0, 0.0),
            Arrangement::SpaceAround if count > 0 => {
                let gap = free / count as f32;
                (gap / 2.0, gap)
            }
            Arrangement::SpaceAround => (0.0, 0.0),
            Arrangement::SpaceEvenly => {
                let gap = free / (count + 1) as f32;
                (gap, gap)
            }
            Arrangement::SpacedBy(spacing) => (0.0, spacing),
        };

        let mut position = start;
        let mut positions = Vec::with_capacity(count);
        for index in 0..count {
            // In a column the cross axis is horizontal, so start and end swap when mirrored.
            let alignment = match (self.alignment, self.vertical && self.rtl) {
                (Alignment::Start, true) => Alignment::End,
                (Alignment::End, true) => Alignment::Start,
                (other, _) => other,
            };
            let across = match alignment {
                Alignment::Start => 0.0,
                Alignment::Center => (cross_size - crosses[index]) / 2.0,
                Alignment::End => cross_size - crosses[index],
            };
            let (x, y) = if self.vertical {
                (across, position)
            } else if self.rtl {
                // A mirrored row starts at its right edge.
                (main_size - position - mains[index], across)
            } else {
                (position, across)
            };
            positions.push(Offset::new(x, y));
            position += mains[index] + gap;
        }

        LayoutResult {
            size: self.join(main_size, cross_size),
            positions,
        }
    }
}
