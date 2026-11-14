//! Tests of the lazy lists over time: placement, scrolling and sliding, with the frame clock
//! advanced by hand and every item's rectangle read back from a real layout.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use rustle_animation::{AnimationSpec, Easing};
use rustle_geometry::{Rect, Size};
use rustle_text::{LineLayout, StyledSpan, TextLayout, TextMeasurer, TextStyle};
use rustle_ui::{Composition, Modifier, canvas, composition, display_list};
use rustle_unit::Constraints;

use super::*;

/// A measurer for layouts with no text.
struct NoText;

impl TextMeasurer for NoText {
    /// Returns an empty size.
    fn measure(&mut self, _: &str, _: &TextStyle, _: f32) -> Size {
        Size::new(0.0, 0.0)
    }

    /// Returns an empty size.
    fn measure_spans(&mut self, _: &[StyledSpan], _: &TextStyle, _: f32) -> Size {
        Size::new(0.0, 0.0)
    }

    /// Returns one empty line.
    fn layout(&mut self, spans: &[StyledSpan], style: &TextStyle, _: f32) -> TextLayout {
        let line = LineLayout {
            start: 0,
            end: 0,
            top: 0.0,
            height: 0.0,
            baseline: 0.0,
            width: 0.0,
            rtl: false,
            hard_break: true,
            clusters: Vec::new(),
        };
        TextLayout::new(String::new(), spans.to_vec(), style.font_size, vec![line])
    }
}

/// Where each probed item was placed in the latest layout, by name.
type Found = Rc<RefCell<BTreeMap<usize, Rect>>>;

/// A composition laid out in a window whose clock the test moves.
struct Stage {
    composition: Composition,
    window: Size,
    found: Found,
    now: Duration,
}

impl Stage {
    /// Composes `root` in a window of `width` by `height` and settles layout.
    fn new(width: f32, height: f32, found: &Found, root: impl FnMut() + 'static) -> Self {
        let mut composition = composition(root);
        composition.compose();
        let mut stage = Self {
            composition,
            window: Size::new(width, height),
            found: Rc::clone(found),
            now: Duration::ZERO,
        };
        stage.settle();
        stage
    }

    /// Lays out and recomposes until nothing changes, as the event loop does.
    fn settle(&mut self) {
        for _ in 0..30 {
            self.found.borrow_mut().clear();
            let tree = self.composition.output().cloned().expect("composed");
            display_list(&tree, Constraints::loose(self.window), &mut NoText);
            if !self.composition.recompose() {
                return;
            }
        }
        panic!("layout does not settle");
    }

    /// Runs one frame `milliseconds` after the last and settles.
    fn frame(&mut self, milliseconds: u64) {
        self.now += Duration::from_millis(milliseconds);
        self.composition.run_frame(self.now);
        self.composition.recompose();
        self.settle();
    }

    /// Runs frames of 16 ms for `milliseconds`.
    fn run(&mut self, milliseconds: u64) {
        let end = self.now + Duration::from_millis(milliseconds);
        while self.now < end {
            self.frame(16.min((end - self.now).as_millis() as u64));
        }
    }

    /// Recomposes and settles after the test changed something the root reads.
    fn touch(&mut self) {
        self.composition.invalidate();
        self.composition.recompose();
        self.settle();
    }

    /// The top of item `index` along `y`, if it is composed.
    fn y(&self, index: usize) -> Option<f32> {
        self.found.borrow().get(&index).map(|rect| rect.origin.y)
    }

    /// The left of item `index`, if it is composed.
    fn x(&self, index: usize) -> Option<f32> {
        self.found.borrow().get(&index).map(|rect| rect.origin.x)
    }

    /// The rectangle of item `index`, if it is composed.
    fn rect(&self, index: usize) -> Option<Rect> {
        self.found.borrow().get(&index).copied()
    }

    /// The indices of the composed items.
    fn items(&self) -> Vec<usize> {
        self.found.borrow().keys().copied().collect()
    }
}

/// Emits an empty area that records its rectangle under `index`.
fn probe(found: &Found, index: usize, modifier: Modifier) {
    let found = Rc::clone(found);
    canvas(
        modifier.on_globally_positioned(move |rect| {
            found.borrow_mut().insert(index, rect);
        }),
        |_| {},
    );
}

/// Asserts `a` is within a quarter pixel of `b`.
#[track_caller]
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.25, "{a} is not near {b}");
}

/// A vertical list of items with `heights`, in a 100 by 200 window.
fn vertical(heights: Rc<RefCell<Vec<f32>>>, content: LazyContent) -> (Stage, LazyListState) {
    let found = Found::default();
    let kept: Rc<RefCell<Option<LazyListState>>> = Rc::default();
    let (probes, state_slot) = (Rc::clone(&found), Rc::clone(&kept));
    let stage = Stage::new(100.0, 200.0, &found, move || {
        let state = remember_lazy_list_state();
        *state_slot.borrow_mut() = Some(state.clone());
        let list = heights.borrow().clone();
        let probes = Rc::clone(&probes);
        lazy_list_with(
            Modifier::new().size(100.0, 200.0),
            &state,
            content,
            list.len(),
            |index| index,
            move |index| {
                probe(
                    &probes,
                    index,
                    Modifier::new().fill_max_width().height(list[index]),
                )
            },
        );
    });
    let state = kept.borrow().clone().expect("state");
    (stage, state)
}

#[test]
fn a_list_places_items_by_their_heights_with_padding_and_spacing() {
    let heights = Rc::new(RefCell::new(vec![40.0; 20]));
    let content = LazyContent::new().padding_each(10.0, 6.0).spacing(5.0);
    let (stage, state) = vertical(heights, content);
    near(stage.y(0).unwrap(), 10.0);
    near(stage.y(1).unwrap(), 55.0);
    near(stage.y(2).unwrap(), 100.0);
    near(stage.y(3).unwrap(), 145.0);
    near(stage.rect(3).unwrap().size.height, 40.0);
    assert_eq!(state.scroll().offset(), 0.0);
    near(
        state.scroll().content_length(),
        10.0 + 20.0 * 45.0 - 5.0 + 6.0,
    );
}

#[test]
fn scrolling_composes_only_the_items_near_the_viewport() {
    let heights = Rc::new(RefCell::new(vec![40.0; 100]));
    let (mut stage, state) = vertical(heights, LazyContent::new());
    assert!(stage.items().len() < 12, "{:?}", stage.items());
    state.scroll().scroll_to(1000.0);
    stage.settle();
    near(stage.y(25).unwrap(), 0.0);
    near(stage.y(27).unwrap(), 80.0);
    assert!(stage.y(0).is_none());
    let (first, last) = (
        *stage.items().first().unwrap(),
        *stage.items().last().unwrap(),
    );
    assert!(first >= 23 && last <= 31, "{first}..{last}");
}

#[test]
fn scroll_to_item_aligns_the_item_as_asked() {
    let heights = Rc::new(RefCell::new(vec![40.0; 50]));
    let (mut stage, state) = vertical(heights, LazyContent::new());
    state.scroll_to_item(10, ItemAlign::Start);
    stage.settle();
    near(stage.y(10).unwrap(), 0.0);
    state.scroll_to_item(10, ItemAlign::Center);
    stage.settle();
    near(stage.y(10).unwrap(), 80.0);
    state.scroll_to_item(10, ItemAlign::End);
    stage.settle();
    near(stage.y(10).unwrap(), 160.0);
    state.scroll_to_item(1000, ItemAlign::Start);
    stage.settle();
    near(state.scroll().offset(), state.scroll().max_offset());
    state.scroll_to_item(0, ItemAlign::Start);
    stage.settle();
    near(stage.y(0).unwrap(), 0.0);
}

#[test]
fn scroll_to_item_with_padding_and_spacing_lands_on_the_item() {
    let heights = Rc::new(RefCell::new(vec![40.0; 50]));
    let content = LazyContent::new().padding(10.0).spacing(10.0);
    let (mut stage, state) = vertical(heights, content);
    state.scroll_to_item(0, ItemAlign::Start);
    stage.settle();
    near(stage.y(0).unwrap(), 10.0);
    state.scroll_to_item(10, ItemAlign::Start);
    stage.settle();
    near(stage.y(10).unwrap(), 0.0);
    state.scroll_to_item(10, ItemAlign::Center);
    stage.settle();
    near(stage.y(10).unwrap(), 80.0);
    state.scroll_to_item(10, ItemAlign::End);
    stage.settle();
    near(stage.y(10).unwrap(), 160.0);
    state.scroll_to_end();
    stage.settle();
    near(stage.y(49).unwrap(), 200.0 - 10.0 - 40.0);
}

#[test]
fn scroll_to_item_reaches_items_with_unmeasured_neighbours() {
    let sizes: Vec<f32> = (0..60)
        .map(|index| 20.0 + (index % 7) as f32 * 10.0)
        .collect();
    let heights = Rc::new(RefCell::new(sizes));
    let (mut stage, state) = vertical(heights, LazyContent::new());
    state.scroll_to_item(40, ItemAlign::Start);
    stage.settle();
    near(stage.y(40).unwrap(), 0.0);
    stage.run(200);
    near(stage.y(40).unwrap(), 0.0);
}

/// A linear slide of `milliseconds`.
fn linear(milliseconds: u64) -> AnimationSpec {
    AnimationSpec::Tween {
        duration: Duration::from_millis(milliseconds),
        easing: Easing::Linear,
    }
}

/// The keys a keyed list shows, shared with the test that changes them.
type Keys = Rc<RefCell<Vec<usize>>>;

/// A keyed vertical list of items 40 tall in a 100 by 200 window, probing each by its key.
fn keyed(keys: &[usize], content: LazyContent) -> (Stage, LazyListState, Keys) {
    let found = Found::default();
    let order: Keys = Rc::new(RefCell::new(keys.to_vec()));
    let kept: Rc<RefCell<Option<LazyListState>>> = Rc::default();
    let (probes, shown, slot) = (Rc::clone(&found), Rc::clone(&order), Rc::clone(&kept));
    let stage = Stage::new(100.0, 200.0, &found, move || {
        let state = remember_lazy_list_state();
        *slot.borrow_mut() = Some(state.clone());
        let list = shown.borrow().clone();
        let probes = Rc::clone(&probes);
        lazy_list_with(
            Modifier::new().size(100.0, 200.0),
            &state,
            content,
            list.len(),
            {
                let list = list.clone();
                move |index| list[index]
            },
            move |index| {
                probe(
                    &probes,
                    list[index],
                    Modifier::new().fill_max_width().height(40.0),
                );
            },
        );
    });
    let state = kept.borrow().clone().expect("state");
    (stage, state, order)
}

#[test]
fn inserting_above_keeps_the_view_and_removing_does_too() {
    let keys: Vec<usize> = (0..50).collect();
    let (mut stage, state, order) = keyed(&keys, LazyContent::new());
    state.scroll().scroll_to(400.0);
    stage.settle();
    near(stage.y(10).unwrap(), 0.0);
    order.borrow_mut().splice(0..0, [100, 101, 102]);
    stage.touch();
    near(stage.y(10).unwrap(), 0.0);
    order.borrow_mut().retain(|key| *key != 100 && *key != 5);
    stage.touch();
    near(stage.y(10).unwrap(), 0.0);
}

#[test]
fn animated_items_slide_to_their_new_places_when_reordered() {
    let content = LazyContent::new().animate_item(linear(100));
    let (mut stage, _, order) = keyed(&[0, 1, 2, 3, 4], content);
    near(stage.y(1).unwrap(), 40.0);
    order.borrow_mut().swap(1, 3);
    stage.touch();
    near(stage.y(1).unwrap(), 40.0);
    near(stage.y(3).unwrap(), 120.0);
    stage.frame(16);
    stage.frame(50);
    near(stage.y(1).unwrap(), 40.0 + 80.0 * 0.5);
    near(stage.y(3).unwrap(), 120.0 - 80.0 * 0.5);
    near(stage.y(2).unwrap(), 80.0);
    stage.frame(60);
    near(stage.y(1).unwrap(), 120.0);
    near(stage.y(3).unwrap(), 40.0);
    stage.run(300);
    near(stage.y(1).unwrap(), 120.0);
    near(stage.y(3).unwrap(), 40.0);
    assert!(!stage.composition.has_pending_frame());
}

#[test]
fn animated_items_slide_over_an_insert_and_a_remove() {
    let content = LazyContent::new().animate_item(linear(100));
    let (mut stage, _, order) = keyed(&[0, 1, 2], content);
    order.borrow_mut().insert(1, 9);
    stage.touch();
    near(stage.y(9).unwrap(), 40.0);
    near(stage.y(1).unwrap(), 40.0);
    stage.frame(16);
    stage.frame(50);
    near(stage.y(1).unwrap(), 60.0);
    near(stage.y(2).unwrap(), 100.0);
    stage.run(200);
    near(stage.y(1).unwrap(), 80.0);
    near(stage.y(2).unwrap(), 120.0);
    order.borrow_mut().retain(|key| *key != 9);
    stage.touch();
    assert!(stage.y(9).is_none());
    stage.frame(16);
    stage.frame(50);
    near(stage.y(1).unwrap(), 60.0);
    stage.run(200);
    near(stage.y(1).unwrap(), 40.0);
    near(stage.y(2).unwrap(), 80.0);
}

#[test]
fn the_view_stays_put_while_items_above_slide() {
    let content = LazyContent::new().animate_item(linear(100));
    let keys: Vec<usize> = (0..50).collect();
    let (mut stage, state, order) = keyed(&keys, content);
    state.scroll().scroll_to(400.0);
    stage.settle();
    stage.run(200);
    near(stage.y(10).unwrap(), 0.0);
    order.borrow_mut().splice(0..0, [100, 101]);
    stage.touch();
    for _ in 0..10 {
        near(stage.y(10).unwrap(), 0.0);
        stage.frame(16);
    }
    stage.run(200);
    near(stage.y(10).unwrap(), 0.0);
}

/// A horizontal keyed list of items with `widths`, in a 200 by 100 window.
fn sideways(widths: Vec<f32>, content: LazyContent) -> (Stage, LazyListState, Keys) {
    let found = Found::default();
    let order: Keys = Rc::new(RefCell::new((0..widths.len()).collect()));
    let kept: Rc<RefCell<Option<LazyListState>>> = Rc::default();
    let (probes, shown, slot) = (Rc::clone(&found), Rc::clone(&order), Rc::clone(&kept));
    let stage = Stage::new(200.0, 100.0, &found, move || {
        let state = remember_lazy_list_state();
        *slot.borrow_mut() = Some(state.clone());
        let list = shown.borrow().clone();
        let widths = widths.clone();
        let probes = Rc::clone(&probes);
        lazy_horizontal_list_with(
            Modifier::new().size(200.0, 100.0),
            &state,
            content,
            list.len(),
            {
                let list = list.clone();
                move |index| list[index]
            },
            move |index| {
                let width = widths[list[index]];
                probe(
                    &probes,
                    list[index],
                    Modifier::new().fill_max_height().width(width),
                );
            },
        );
    });
    let state = kept.borrow().clone().expect("state");
    (stage, state, order)
}

#[test]
fn a_horizontal_list_places_scrolls_and_slides_along_x() {
    let widths: Vec<f32> = (0..40)
        .map(|index| 30.0 + (index % 3) as f32 * 10.0)
        .collect();
    let content = LazyContent::new()
        .padding(8.0)
        .spacing(4.0)
        .animate_item(linear(100));
    let (mut stage, state, order) = sideways(widths, content);
    near(stage.x(0).unwrap(), 8.0);
    near(stage.x(1).unwrap(), 8.0 + 34.0);
    near(stage.x(2).unwrap(), 8.0 + 34.0 + 44.0);
    assert!(stage.items().len() < 14);
    state.scroll_to_item(20, ItemAlign::Start);
    stage.run(100);
    near(stage.x(20).unwrap(), 0.0);
    state.scroll_to_item(20, ItemAlign::End);
    stage.run(100);
    near(
        stage.x(20).unwrap() + stage.rect(20).unwrap().size.width,
        200.0,
    );
    state.scroll_to_item(0, ItemAlign::Start);
    stage.run(100);
    near(stage.x(0).unwrap(), 8.0);
    order.borrow_mut().swap(0, 2);
    stage.touch();
    let (a, b) = (stage.x(0).unwrap(), stage.x(2).unwrap());
    stage.frame(16);
    stage.frame(50);
    let (c, d) = (stage.x(0).unwrap(), stage.x(2).unwrap());
    assert!(c > a && d < b, "{a}->{c} {b}->{d}");
    stage.run(300);
    assert!(stage.x(2).unwrap() < stage.x(0).unwrap());
    near(stage.x(2).unwrap(), 8.0);
    state.scroll_to_end();
    stage.run(100);
    near(state.scroll().offset(), state.scroll().max_offset());
}

#[test]
fn a_list_keeps_its_end_padding_at_the_end() {
    let (mut stage, state, _) =
        sideways(vec![50.0; 10], LazyContent::new().padding_each(0.0, 25.0));
    state.scroll_to_end();
    stage.run(100);
    near(stage.x(9).unwrap(), 200.0 - 25.0 - 50.0);
}

/// A vertical sticky list where every fifth item is a header 30 tall and the others 40.
fn sticky(content: LazyContent) -> (Stage, LazyListState) {
    let found = Found::default();
    let kept: Rc<RefCell<Option<LazyListState>>> = Rc::default();
    let (probes, slot) = (Rc::clone(&found), Rc::clone(&kept));
    let stage = Stage::new(100.0, 200.0, &found, move || {
        let state = remember_lazy_list_state();
        *slot.borrow_mut() = Some(state.clone());
        let probes = Rc::clone(&probes);
        lazy_sticky_list(
            Modifier::new().size(100.0, 200.0),
            &state,
            content,
            40,
            |index| index,
            |index| index % 5 == 0,
            move |index| {
                let height = if index % 5 == 0 { 30.0 } else { 40.0 };
                probe(
                    &probes,
                    index,
                    Modifier::new().fill_max_width().height(height),
                );
            },
        );
    });
    let state = kept.borrow().clone().expect("state");
    (stage, state)
}

#[test]
fn a_sticky_header_stays_then_is_pushed_up_by_the_next() {
    let (mut stage, state) = sticky(LazyContent::new());
    near(stage.y(0).unwrap(), 0.0);
    state.scroll().scroll_to(50.0);
    stage.run(50);
    near(stage.y(0).unwrap(), 0.0);
    near(stage.y(1).unwrap(), -20.0);
    state.scroll().scroll_to(30.0 + 4.0 * 40.0 - 10.0);
    stage.run(50);
    let top = 30.0 + 4.0 * 40.0 - state.scroll().offset();
    near(stage.y(5).unwrap(), top);
    near(stage.y(0).unwrap(), top - 30.0);
    state.scroll().scroll_to(30.0 + 4.0 * 40.0 + 5.0);
    stage.run(50);
    near(stage.y(5).unwrap(), 0.0);
    state.scroll().scroll_to(900.0);
    stage.run(50);
    near(stage.y(20).unwrap(), 0.0);
    near(stage.y(25).unwrap(), 950.0 - 900.0);
}

#[test]
fn a_sticky_header_is_pushed_exactly_with_spacing() {
    let (mut stage, state) = sticky(LazyContent::new().spacing(10.0));
    let next = 30.0 + 10.0 + 4.0 * 50.0;
    state.scroll().scroll_to(next - 20.0);
    stage.run(50);
    near(stage.y(0).unwrap(), -10.0);
    near(stage.y(5).unwrap(), 20.0);
}

/// A vertical staggered grid of two lanes in a 100 by 200 window; item heights follow `height`.
fn staggered(
    height: impl Fn(usize) -> f32 + 'static,
    count: usize,
    content: LazyContent,
) -> (Stage, LazyListState, Keys) {
    let found = Found::default();
    let order: Keys = Rc::new(RefCell::new((0..count).collect()));
    let kept: Rc<RefCell<Option<LazyListState>>> = Rc::default();
    let (probes, shown, slot) = (Rc::clone(&found), Rc::clone(&order), Rc::clone(&kept));
    let height = Rc::new(height);
    let stage = Stage::new(100.0, 200.0, &found, move || {
        let state = remember_lazy_list_state();
        *slot.borrow_mut() = Some(state.clone());
        let list = shown.borrow().clone();
        let (probes, height) = (Rc::clone(&probes), Rc::clone(&height));
        lazy_vertical_staggered_grid(
            Modifier::new().size(100.0, 200.0),
            &state,
            content,
            2,
            list.len(),
            {
                let list = list.clone();
                move |index| list[index]
            },
            move |index| {
                let id = list[index];
                probe(
                    &probes,
                    id,
                    Modifier::new().fill_max_width().height(height(id)),
                );
            },
        );
    });
    let state = kept.borrow().clone().expect("state");
    (stage, state, order)
}

#[test]
fn a_staggered_grid_deals_into_the_shortest_lane() {
    let (stage, _, _) = staggered(
        |id| [30.0, 50.0, 40.0, 20.0, 60.0, 10.0][id % 6],
        12,
        LazyContent::new(),
    );
    near(stage.x(0).unwrap(), 0.0);
    near(stage.x(1).unwrap(), 50.0);
    near(stage.y(0).unwrap(), 0.0);
    near(stage.y(1).unwrap(), 0.0);
    near(stage.x(2).unwrap(), 0.0);
    near(stage.y(2).unwrap(), 30.0);
    near(stage.x(3).unwrap(), 50.0);
    near(stage.y(3).unwrap(), 50.0);
    near(stage.x(4).unwrap(), 0.0);
    near(stage.y(4).unwrap(), 70.0);
    near(stage.x(5).unwrap(), 50.0);
    near(stage.y(5).unwrap(), 70.0);
}

#[test]
fn a_staggered_grid_applies_padding_spacing_and_scrolls_to_its_end() {
    let content = LazyContent::new().padding_each(6.0, 14.0).spacing(4.0);
    let (mut stage, state, _) = staggered(|_| 36.0, 10, content);
    near(stage.y(0).unwrap(), 6.0);
    near(stage.y(2).unwrap(), 46.0);
    state.scroll().scroll_to(10_000.0);
    stage.run(50);
    near(state.scroll().offset(), state.scroll().max_offset());
    let last = stage.rect(9).unwrap();
    near(last.origin.y + last.size.height + 4.0 + 14.0, 200.0);
}

#[test]
fn a_staggered_grid_slides_items_that_change_lane_position_on_remove() {
    let content = LazyContent::new().animate_item(linear(100));
    let (mut stage, _, order) = staggered(|_| 40.0, 6, content);
    near(stage.y(2).unwrap(), 40.0);
    order.borrow_mut().retain(|id| *id != 0);
    stage.touch();
    near(stage.y(2).unwrap(), 40.0);
    stage.frame(16);
    stage.frame(50);
    near(stage.y(2).unwrap(), 20.0);
    stage.run(200);
    near(stage.y(2).unwrap(), 0.0);
    assert!(stage.y(0).is_none());
}

#[test]
fn a_staggered_grid_holds_the_view_when_items_above_are_measured() {
    let (mut stage, state, _) =
        staggered(|id| 30.0 + (id % 5) as f32 * 11.0, 60, LazyContent::new());
    state.scroll().scroll_to(300.0);
    stage.run(50);
    let before = stage
        .items()
        .into_iter()
        .map(|id| (id, stage.y(id).unwrap()))
        .collect::<Vec<_>>();
    stage.run(300);
    for (id, y) in before {
        if let Some(now) = stage.y(id) {
            near(now, y);
        }
    }
}

/// A spanned grid of three columns in a 120 by 200 window: item 0 spans all, item 3 spans two.
fn spanned(count: usize) -> (Stage, LazyListState, Keys) {
    let found = Found::default();
    let order: Keys = Rc::new(RefCell::new((0..count).collect()));
    let kept: Rc<RefCell<Option<LazyListState>>> = Rc::default();
    let (probes, shown, slot) = (Rc::clone(&found), Rc::clone(&order), Rc::clone(&kept));
    let stage = Stage::new(120.0, 200.0, &found, move || {
        let state = remember_lazy_list_state();
        *slot.borrow_mut() = Some(state.clone());
        let list = shown.borrow().clone();
        let probes = Rc::clone(&probes);
        lazy_vertical_grid_spanned(
            Modifier::new().size(120.0, 200.0),
            &state,
            LazyContent::new().spacing(6.0).padding(4.0),
            3,
            list.len(),
            {
                let list = list.clone();
                move |index| list[index]
            },
            |index| match index {
                0 => 3,
                3 => 2,
                _ => 1,
            },
            move |index| {
                let height = if index == 0 { 20.0 } else { 40.0 };
                probe(
                    &probes,
                    index,
                    Modifier::new().fill_max_width().height(height),
                );
            },
        );
    });
    let state = kept.borrow().clone().expect("state");
    (stage, state, order)
}

#[test]
fn a_spanned_grid_packs_rows_by_span_and_sizes_columns_by_weight() {
    let (mut stage, state, _) = spanned(20);
    let zero = stage.rect(0).unwrap();
    near(zero.origin.y, 4.0);
    near(zero.size.width, 120.0);
    let (one, two) = (stage.rect(1).unwrap(), stage.rect(2).unwrap());
    near(one.origin.y, 4.0 + 20.0 + 6.0);
    near(two.origin.y, one.origin.y);
    near(one.size.width, 40.0);
    near(two.origin.x, 40.0);
    let (three, four) = (stage.rect(3).unwrap(), stage.rect(4).unwrap());
    near(three.origin.y, one.origin.y + 40.0 + 6.0);
    near(three.size.width, 80.0);
    near(four.origin.x, 80.0);
    near(four.origin.y, three.origin.y);
    state.scroll_to_item(4, ItemAlign::Start);
    stage.run(100);
    assert!(
        stage.y(4).is_some_and(|y| y.abs() < 0.5),
        "{:?}",
        stage.y(4)
    );
}
