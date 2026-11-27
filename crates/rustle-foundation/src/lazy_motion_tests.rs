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
    assert!(first >= 20 && last <= 34, "{first}..{last}");
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

/// A list state made outside composition, as a state holder owns it, starts where it was
/// told, follows scrolling done from outside, and is the same state in every pass.
#[test]
fn a_list_state_made_outside_composition_drives_the_list() {
    let found = Found::default();
    let state = LazyListState::new(10, 0.0);
    let (probes, shown) = (Rc::clone(&found), state.clone());
    let mut stage = Stage::new(100.0, 200.0, &found, move || {
        let probes = Rc::clone(&probes);
        lazy_list_with(
            Modifier::new().size(100.0, 200.0),
            &shown,
            LazyContent::new(),
            100,
            |index| index,
            move |index| {
                probe(
                    &probes,
                    index,
                    Modifier::new().fill_max_width().height(40.0),
                )
            },
        );
    });
    stage.settle();
    near(stage.y(10).unwrap(), 0.0);
    state.scroll().scroll_to(800.0);
    stage.settle();
    near(stage.y(20).unwrap(), 0.0);
    assert_eq!(state.clone(), state);
    assert_ne!(state, LazyListState::new(0, 0.0));
}

/// The layout info lists the items in view with their places along the viewport.
#[test]
fn layout_info_lists_the_items_in_view() {
    let heights = Rc::new(RefCell::new(vec![40.0; 50]));
    let (mut stage, state) = vertical(heights, LazyContent::new());
    state.scroll_to_item_offset(10, 20.0);
    stage.settle();
    let info = state.layout_info();
    assert_eq!(info.total_items_count, 50);
    near(info.viewport_end_offset, 200.0);
    let indices: Vec<usize> = info
        .visible_items_info
        .iter()
        .map(|item| item.index)
        .collect();
    assert_eq!(indices, (10..=15).collect::<Vec<_>>());
    near(info.visible_items_info[0].offset, -20.0);
    near(info.visible_items_info[1].offset, 20.0);
    near(info.visible_items_info[0].size, 40.0);
}

/// A list of 100 items 40 tall in a 100 by 200 window whose item 2 pins itself while
/// `pin` holds true.
fn pinning(pin: Rc<std::cell::Cell<bool>>) -> (Stage, LazyListState) {
    let found = Found::default();
    let kept: Rc<RefCell<Option<LazyListState>>> = Rc::default();
    let (probes, state_slot) = (Rc::clone(&found), Rc::clone(&kept));
    let stage = Stage::new(100.0, 200.0, &found, move || {
        let state = remember_lazy_list_state();
        *state_slot.borrow_mut() = Some(state.clone());
        let probes = Rc::clone(&probes);
        let pinned = pin.get();
        lazy_list_keyed(
            Modifier::new().size(100.0, 200.0),
            &state,
            100,
            |index| index,
            move |index| {
                if index == 2 {
                    let container = LOCAL_PINNABLE_CONTAINER.current().expect("inside an item");
                    rustle_runtime::disposable_effect(pinned, move || {
                        let handle = pinned.then(|| container.pin());
                        move || drop(handle)
                    });
                }
                probe(
                    &probes,
                    index,
                    Modifier::new().fill_max_width().height(40.0),
                )
            },
        );
    });
    let state = kept.borrow().clone().expect("state");
    (stage, state)
}

#[test]
fn a_pinned_item_stays_composed_at_its_place_while_scrolled_away() {
    let pin = Rc::new(std::cell::Cell::new(true));
    let (mut stage, state) = pinning(Rc::clone(&pin));
    state.scroll().scroll_to(2000.0);
    stage.touch();
    near(stage.y(2).expect("item 2 is pinned"), 80.0 - 2000.0);
    near(stage.y(50).expect("item 50 is in view"), 0.0);
    near(state.scroll().content_length(), 4000.0);
    pin.set(false);
    stage.touch();
    assert_eq!(stage.y(2), None, "{:?}", stage.items());
    near(stage.y(50).expect("item 50 is in view"), 0.0);
}

#[test]
fn a_reversed_list_places_its_first_item_at_the_bottom() {
    let heights = Rc::new(RefCell::new(vec![40.0; 20]));
    let content = LazyContent::new()
        .padding_each(10.0, 6.0)
        .spacing(5.0)
        .reverse_layout(true);
    let (stage, state) = vertical(heights, content);
    near(stage.y(0).unwrap(), 150.0);
    near(stage.y(1).unwrap(), 105.0);
    near(stage.y(2).unwrap(), 60.0);
    assert_eq!(state.scroll().offset(), 0.0);
    assert_eq!(state.first_visible_item(), Some(0));
    near(
        state.scroll().content_length(),
        10.0 + 20.0 * 45.0 - 5.0 + 6.0,
    );
}

#[test]
fn a_reversed_list_keeps_its_first_item_at_the_bottom_as_it_grows_and_items_arrive() {
    let heights = Rc::new(RefCell::new(vec![40.0; 20]));
    let content = LazyContent::new().reverse_layout(true);
    let (mut stage, state) = vertical(Rc::clone(&heights), content);
    near(stage.y(0).unwrap(), 160.0);
    heights.borrow_mut()[0] = 90.0;
    stage.touch();
    near(stage.y(0).unwrap(), 110.0);
    heights.borrow_mut().extend([40.0; 5]);
    stage.touch();
    near(stage.y(0).unwrap(), 110.0);
    assert_eq!(state.scroll().offset(), 0.0);
}

#[test]
fn a_reversed_list_scrolled_back_keeps_what_is_in_view_as_its_first_item_grows() {
    let heights = Rc::new(RefCell::new(vec![40.0; 30]));
    let content = LazyContent::new().reverse_layout(true);
    let (mut stage, state) = vertical(Rc::clone(&heights), content);
    state.scroll().scroll_to(400.0);
    stage.settle();
    let shown = stage.y(12).expect("item 12 is in view");
    assert!(stage.y(0).is_none());
    heights.borrow_mut()[0] = 140.0;
    stage.touch();
    near(stage.y(12).unwrap(), shown);
    state.scroll().scroll_to(0.0);
    stage.settle();
    near(stage.y(0).unwrap(), 60.0);
}

/// A reversed list scrolled as far as it goes keeps the padding after its last item in view,
/// and its offset within the room it measured.
#[test]
fn a_reversed_list_scrolled_to_its_end_shows_the_padding_after_its_last_item() {
    let heights = Rc::new(RefCell::new(vec![40.0; 20]));
    let content = LazyContent::new()
        .padding_each(10.0, 6.0)
        .spacing(5.0)
        .reverse_layout(true);
    let (mut stage, state) = vertical(heights, content);
    state.scroll().scroll_to(f32::MAX);
    stage.settle();
    near(stage.y(19).expect("the last item is in view"), 6.0);
    near(state.scroll().offset(), state.scroll().max_offset());
}

/// A scroll that keeps the viewport inside what the list composed composes nothing again;
/// one that leaves it composes the list once more, around the new viewport. The state is
/// held outside the composition, as a state holder holds it, so only its readers follow it.
#[test]
fn scrolling_within_the_composed_items_composes_nothing() {
    let found = Found::default();
    let runs = Rc::new(std::cell::Cell::new(0));
    let state = LazyListState::default();
    let (probes, held, counted) = (Rc::clone(&found), state.clone(), Rc::clone(&runs));
    let mut stage = Stage::new(100.0, 200.0, &found, move || {
        counted.set(counted.get() + 1);
        let state = held.clone();
        let probes = Rc::clone(&probes);
        lazy_list_with(
            Modifier::new().size(100.0, 200.0),
            &state,
            LazyContent::new(),
            100,
            |index| index,
            move |index| {
                probe(
                    &probes,
                    index,
                    Modifier::new().fill_max_width().height(40.0),
                )
            },
        );
    });
    let settled = runs.get();
    state.scroll().scroll_to(40.0);
    stage.settle();
    assert_eq!(runs.get(), settled, "a scroll inside what is composed");
    near(stage.y(1).unwrap(), 0.0);
    state.scroll().scroll_to(1000.0);
    stage.settle();
    assert!(runs.get() > settled, "a scroll past what is composed");
    near(stage.y(25).unwrap(), 0.0);
}

/// An item at the start of the viewport of a reversed list growing, as the newest message of
/// a chat does while it streams in, is taken in during layout: the list composes nothing
/// again, the item stays at the bottom and the length of the list follows.
#[test]
fn a_growing_item_in_view_composes_the_list_nothing_again() {
    let found = Found::default();
    let listed = Rc::new(std::cell::Cell::new(0));
    let grown = rustle_runtime::mutable_state(40.0_f32);
    let state = LazyListState::default();
    let (probes, held, counted, height) = (
        Rc::clone(&found),
        state.clone(),
        Rc::clone(&listed),
        grown.clone(),
    );
    let mut stage = Stage::new(100.0, 200.0, &found, move || {
        let state = held.clone();
        let probes = Rc::clone(&probes);
        let counted = Rc::clone(&counted);
        let height = height.clone();
        lazy_list_with(
            Modifier::new().size(100.0, 200.0),
            &state,
            LazyContent::new().reverse_layout(true),
            20,
            move |index| {
                if index == 0 {
                    counted.set(counted.get() + 1);
                }
                index
            },
            move |index| {
                let height = height.clone();
                let probes = Rc::clone(&probes);
                rustle_ui::restartable(index, move || {
                    let length = if index == 0 { height.get() } else { 40.0 };
                    probe(
                        &probes,
                        index,
                        Modifier::new().fill_max_width().height(length),
                    )
                })
            },
        );
    });
    near(stage.y(0).unwrap(), 160.0);
    let settled = listed.get();
    let length = state.scroll().content_length();
    grown.set(90.0);
    stage.frame(16);
    assert_eq!(listed.get(), settled, "the list composed again");
    near(stage.y(0).unwrap(), 110.0);
    near(stage.y(1).unwrap(), 70.0);
    near(state.scroll().content_length(), length + 50.0);
    assert_eq!(state.scroll().offset(), 0.0);
    assert_eq!(
        state.layout_info().visible_items_info[0],
        LazyListItemInfo {
            index: 0,
            offset: 0.0,
            size: 90.0
        }
    );
}

/// A uniform list of `count` items 20 tall with `content`, in a 100 by 200 window.
fn uniform(count: usize, content: LazyContent) -> (Stage, LazyListState) {
    let found = Found::default();
    let kept: Rc<RefCell<Option<LazyListState>>> = Rc::default();
    let (probes, state_slot) = (Rc::clone(&found), Rc::clone(&kept));
    let stage = Stage::new(100.0, 200.0, &found, move || {
        let state = remember_lazy_list_state();
        *state_slot.borrow_mut() = Some(state.clone());
        let probes = Rc::clone(&probes);
        uniform_list(
            Modifier::new().size(100.0, 200.0),
            &state,
            content,
            count,
            20.0,
            move |index| probe(&probes, index, Modifier::new().fill_max_size()),
        );
    });
    let state = kept.borrow().clone().expect("state");
    (stage, state)
}

/// Rows of a list of items that take one, two or no rows in turn, the third hidden.
struct Varied {
    /// The first row of each item and, last, how many rows there are.
    starts: Vec<usize>,
}

impl Varied {
    /// `count` items taking one, two and no rows in turn.
    fn new(count: usize) -> Self {
        let mut starts = vec![0];
        for index in 0..count {
            let rows = [1, 2, 0][index % 3];
            starts.push(starts[index] + rows);
        }
        Self { starts }
    }
}

impl UniformRows for Varied {
    /// How many items there are.
    fn items(&self) -> usize {
        self.starts.len() - 1
    }

    /// How many rows they take.
    fn rows(&self) -> usize {
        self.starts[self.starts.len() - 1]
    }

    /// The first row of `item`.
    fn first_row(&self, item: usize) -> usize {
        self.starts[item.min(self.items())]
    }

    /// The item drawn in `row`.
    fn item_at_row(&self, row: usize) -> usize {
        if row >= self.rows() {
            return self.items();
        }
        self.starts.partition_point(|start| *start <= row) - 1
    }
}

#[test]
fn a_uniform_list_places_items_on_rows_and_composes_only_those_near_the_view() {
    let (mut stage, state) = uniform(100_000, LazyContent::new().padding_each(5.0, 7.0));
    near(stage.y(0).unwrap(), 5.0);
    near(stage.y(3).unwrap(), 65.0);
    near(stage.rect(3).unwrap().size.height, 20.0);
    near(
        state.scroll().content_length(),
        5.0 + 100_000.0 * 20.0 + 7.0,
    );
    assert!(stage.items().len() <= 16, "{:?}", stage.items());
    state.scroll().scroll_to(5.0 + 50_000.0 * 20.0 + 4.0);
    stage.settle();
    near(stage.y(50_000).unwrap(), -4.0);
    near(stage.y(50_001).unwrap(), 16.0);
    assert!(stage.y(0).is_none());
    let items = stage.items();
    assert!(
        items.len() <= 16 && items[0] >= 49_996 && items[items.len() - 1] <= 50_016,
        "{items:?}"
    );
    assert_eq!(state.first_visible_item(), Some(50_000));
    near(state.first_visible_item_scroll_offset(), 4.0);
    let info = state.layout_info();
    assert_eq!(info.total_items_count, 100_000);
    assert_eq!(info.visible_items_info[0].index, 50_000);
    assert_eq!(info.visible_items_info.len(), 11);
}

#[test]
fn a_uniform_list_scrolls_to_items_as_asked() {
    let (mut stage, state) = uniform(1_000, LazyContent::new().padding(10.0));
    state.scroll_to_item(100, ItemAlign::Start);
    stage.settle();
    near(stage.y(100).unwrap(), 0.0);
    state.scroll_to_item(100, ItemAlign::Center);
    stage.settle();
    near(stage.y(100).unwrap(), 90.0);
    state.scroll_to_item(100, ItemAlign::End);
    stage.settle();
    near(stage.y(100).unwrap(), 180.0);
    state.scroll_to_item_offset(200, 5.0);
    stage.settle();
    near(stage.y(200).unwrap(), -5.0);
    state.scroll_to_end();
    stage.settle();
    near(stage.y(999).unwrap(), 200.0 - 10.0 - 20.0);
    near(state.scroll().offset(), state.scroll().max_offset());
    state.scroll_to_item(5_000, ItemAlign::Start);
    stage.settle();
    near(state.scroll().offset(), state.scroll().max_offset());
    state.scroll_to_item_offset(0, -10.0);
    stage.settle();
    near(state.scroll().offset(), 0.0);
    near(stage.y(0).unwrap(), 10.0);
}

#[test]
fn a_uniform_list_with_rows_gives_items_their_rows_and_skips_hidden_ones() {
    let found = Found::default();
    let kept: Rc<RefCell<Option<LazyListState>>> = Rc::default();
    let (probes, state_slot) = (Rc::clone(&found), Rc::clone(&kept));
    let mut stage = Stage::new(100.0, 200.0, &found, move || {
        let state = remember_lazy_list_state();
        *state_slot.borrow_mut() = Some(state.clone());
        let probes = Rc::clone(&probes);
        uniform_list_with(
            Modifier::new().size(100.0, 200.0),
            &state,
            LazyContent::new(),
            10.0,
            &Varied::new(3_000),
            |index| index,
            move |index| probe(&probes, index, Modifier::new().fill_max_size()),
        );
    });
    let state = kept.borrow().clone().expect("state");
    near(stage.y(1).unwrap(), 10.0);
    near(stage.rect(1).unwrap().size.height, 20.0);
    near(stage.y(3).unwrap(), 30.0);
    near(state.scroll().content_length(), 3_000.0 * 10.0);
    state.scroll_to_item(1_501, ItemAlign::Start);
    stage.settle();
    near(stage.y(1_501).unwrap(), 0.0);
    assert_eq!(state.first_visible_item(), Some(1_501));
    state.scroll().scroll_to(1_501.0 * 10.0 + 15.0);
    stage.settle();
    assert_eq!(state.first_visible_item(), Some(1_501));
    near(state.first_visible_item_scroll_offset(), 15.0);
    let info = state.layout_info();
    assert_eq!(info.visible_items_info[0].index, 1_501);
    assert!(
        info.visible_items_info
            .iter()
            .all(|item| item.index % 3 != 2)
    );
}

impl Stage {
    /// Lays out once, as a frame drawn without settling would, and returns whether the
    /// layout asked the list to compose again: the frame shows what that one pass placed.
    fn single_pass(&mut self) -> bool {
        self.found.borrow_mut().clear();
        let tree = self.composition.output().cloned().expect("composed");
        display_list(&tree, Constraints::loose(self.window), &mut NoText);
        self.composition.recompose()
    }
}

/// How many of `frames` frames, each scrolling `step` and laid out once, drew an item
/// somewhere other than where settling put it, in a list of varied heights scrolled from
/// `start`.
fn stale_frames(start: f32, step: f32, frames: usize, reverse: bool) -> usize {
    let heights = Rc::new(RefCell::new(
        (0..400)
            .map(|index| 20.0 + ((index * 37) % 90) as f32)
            .collect::<Vec<f32>>(),
    ));
    let (mut stage, state) = vertical(heights, LazyContent::new().reverse_layout(reverse));
    state.scroll().scroll_to(start);
    stage.settle();
    let mut stale = 0;
    for _ in 0..frames {
        state.scroll().scroll_by(step);
        stage.composition.recompose();
        stage.single_pass();
        let drawn = stage.found.borrow().clone();
        let offset = state.scroll().offset();
        stage.settle();
        let settled = stage.found.borrow().clone();
        let moved = drawn.iter().any(|(index, rect)| {
            settled
                .get(index)
                .is_some_and(|other| (other.origin.y - rect.origin.y).abs() > 0.25)
                && rect.origin.y < 200.0
                && rect.origin.y + rect.size.height > 0.0
        });
        if moved || (state.scroll().offset() - offset).abs() > 0.25 {
            stale += 1;
        }
    }
    stale
}

/// Items newly in view below take in their measured lengths during the layout that measures
/// them, but those scrolled into view above wait for the next composition, so a frame laid
/// out once while scrolling back up shows them out of place: why a window composes and lays
/// out again before drawing while layout changes what composition reads.
#[test]
fn only_items_measured_above_the_view_need_a_second_pass() {
    assert_eq!(stale_frames(0.0, 13.0, 200, false), 0);
    assert_eq!(stale_frames(0.0, 13.0, 200, true), 0);
    assert!(stale_frames(20_000.0, -13.0, 200, false) > 0);
}
