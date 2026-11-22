//! Drives an [`Embedded`] interface with simulated pointer, contact and wheel events and
//! checks what the gesture modifiers do: drags, anchored drags, transforms, scrolling with
//! nested scroll and fling, the raw pointer stream, double taps and drag sources.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::thread::sleep;
use std::time::Duration;

use rustle::prelude::*;
use rustle::{
    Embedded, LOCAL_SCROLLBAR_STYLE, Offset, PointerButton, PointerId, ScrollbarStyle,
    ScrollbarVisibility,
};

/// A headless interface of 400 by 400 logical pixels with `root` composed in it.
fn rig(root: impl FnMut() + 'static) -> Embedded {
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((400, 400), 1.0);
    embedded.update();
    embedded
}

/// Waits about one frame and runs it.
fn frame(embedded: &mut Embedded) {
    sleep(Duration::from_millis(16));
    embedded.update();
}

/// Runs frames until nothing asks for another one or `limit` frames passed. Returns how many ran.
fn settle(embedded: &mut Embedded, limit: usize) -> usize {
    for count in 0..limit {
        if !embedded.wants_frame() {
            return count;
        }
        frame(embedded);
    }
    limit
}

/// Presses the mouse at `from`, moves in `steps` equal steps to `to` a few milliseconds apart,
/// and returns without releasing.
fn press_and_move(embedded: &mut Embedded, from: Offset, to: Offset, steps: u32) {
    embedded.pointer_moved(from);
    embedded.pointer_button(PointerButton::Primary, true);
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        sleep(Duration::from_millis(8));
        embedded.pointer_moved(Offset::new(
            from.x + (to.x - from.x) * t,
            from.y + (to.y - from.y) * t,
        ));
        embedded.update();
    }
}

/// A drag with the mouse that ends with the button released.
fn drag(embedded: &mut Embedded, from: Offset, to: Offset, steps: u32) {
    press_and_move(embedded, from, to, steps);
    embedded.pointer_button(PointerButton::Primary, false);
    embedded.update();
}

/// The state a root composable made, taken out for the test to read.
type Slot<T> = Rc<RefCell<Option<T>>>;

fn slot<T>() -> Slot<T> {
    Rc::new(RefCell::new(None))
}

fn get<T: Clone>(slot: &Slot<T>) -> T {
    slot.borrow().clone().expect("the root composed")
}

#[test]
fn draggable_follows_the_pointer_along_its_axis() {
    let state = slot();
    let out = state.clone();
    let mut ui = rig(move || {
        let drag = remember_draggable_state();
        *out.borrow_mut() = Some(drag.clone());
        stack_with(
            Modifier::new()
                .size(100.0, 100.0)
                .draggable(&drag, Axis::Horizontal),
            || {},
        );
    });
    let state: DraggableState = get(&state);
    press_and_move(&mut ui, Offset::new(50.0, 50.0), Offset::new(90.0, 80.0), 8);
    assert!(state.is_dragging());
    assert!((state.offset() - 40.0).abs() < 0.01, "{}", state.offset());
    ui.pointer_button(PointerButton::Primary, false);
    assert!(!state.is_dragging());
    assert!(state.velocity() > 20.0, "{}", state.velocity());
    drag(&mut ui, Offset::new(50.0, 50.0), Offset::new(40.0, 50.0), 4);
    assert!((state.offset() - 30.0).abs() < 0.01, "{}", state.offset());
}

#[test]
fn draggable_ignores_movement_under_the_slop() {
    let state = slot();
    let out = state.clone();
    let mut ui = rig(move || {
        let drag = remember_draggable_state();
        *out.borrow_mut() = Some(drag.clone());
        stack_with(
            Modifier::new()
                .size(100.0, 100.0)
                .draggable(&drag, Axis::Vertical),
            || {},
        );
    });
    let state: DraggableState = get(&state);
    drag(&mut ui, Offset::new(50.0, 50.0), Offset::new(80.0, 52.0), 3);
    assert_eq!(state.offset(), 0.0);
    assert!(!state.is_dragging());
}

#[test]
fn draggable_with_reports_deltas_and_release_speed() {
    let deltas = Rc::new(RefCell::new(Vec::new()));
    let stopped = Rc::new(Cell::new(f32::NAN));
    let (d, s) = (deltas.clone(), stopped.clone());
    let mut ui = rig(move || {
        let drag = remember_draggable_state();
        let (d, s) = (d.clone(), s.clone());
        stack_with(
            Modifier::new().size(100.0, 100.0).draggable_with(
                &drag,
                Axis::Vertical,
                move |delta| d.borrow_mut().push(delta),
                move |velocity| s.set(velocity),
            ),
            || {},
        );
    });
    press_and_move(&mut ui, Offset::new(50.0, 20.0), Offset::new(50.0, 60.0), 4);
    sleep(Duration::from_millis(150));
    ui.pointer_button(PointerButton::Primary, false);
    let total: f32 = deltas.borrow().iter().sum();
    assert!((total - 40.0).abs() < 0.01, "{total}");
    assert!(
        stopped.get().abs() < 1.0,
        "a pause before release is no fling"
    );
}

#[test]
fn draggable_keeps_following_outside_the_area_and_after_leaving_the_window() {
    let state = slot();
    let out = state.clone();
    let mut ui = rig(move || {
        let drag = remember_draggable_state();
        *out.borrow_mut() = Some(drag.clone());
        stack_with(
            Modifier::new()
                .size(100.0, 100.0)
                .draggable(&drag, Axis::Horizontal),
            || {},
        );
    });
    let state: DraggableState = get(&state);
    press_and_move(
        &mut ui,
        Offset::new(50.0, 50.0),
        Offset::new(300.0, 50.0),
        5,
    );
    assert!((state.offset() - 250.0).abs() < 0.01);
    ui.pointer_left();
    ui.pointer_button(PointerButton::Primary, false);
    assert!(
        !state.is_dragging(),
        "a release after the pointer left ends the drag"
    );
    drag(&mut ui, Offset::new(50.0, 50.0), Offset::new(70.0, 50.0), 2);
    assert!((state.offset() - 270.0).abs() < 0.01, "{}", state.offset());
}

#[test]
fn anchored_draggable_settles_on_the_nearest_anchor() {
    let state = slot();
    let out = state.clone();
    let mut ui = rig(move || {
        let anchored = remember_anchored_draggable_state(vec![0.0, 100.0, 200.0], 0);
        *out.borrow_mut() = Some(anchored.clone());
        stack_with(
            Modifier::new()
                .size(300.0, 50.0)
                .anchored_draggable(&anchored, Axis::Horizontal),
            || {},
        );
    });
    let state: AnchoredDraggableState = get(&state);
    press_and_move(
        &mut ui,
        Offset::new(10.0, 10.0),
        Offset::new(130.0, 10.0),
        6,
    );
    sleep(Duration::from_millis(150));
    assert!((state.offset() - 120.0).abs() < 0.01);
    assert!(state.is_dragging());
    ui.pointer_button(PointerButton::Primary, false);
    assert!(state.is_settling());
    assert_eq!(state.target(), 1);
    assert!(settle(&mut ui, 300) < 300);
    assert_eq!(state.offset(), 100.0);
    assert!(!state.is_settling());
}

#[test]
fn anchored_draggable_flings_to_the_next_anchor_and_stays_in_bounds() {
    let state = slot();
    let out = state.clone();
    let mut ui = rig(move || {
        let anchored = remember_anchored_draggable_state(vec![0.0, 100.0, 200.0], 0);
        *out.borrow_mut() = Some(anchored.clone());
        stack_with(
            Modifier::new()
                .size(300.0, 50.0)
                .anchored_draggable(&anchored, Axis::Horizontal),
            || {},
        );
    });
    let state: AnchoredDraggableState = get(&state);
    drag(&mut ui, Offset::new(10.0, 10.0), Offset::new(40.0, 10.0), 3);
    settle(&mut ui, 300);
    assert_eq!(state.target(), 1, "a quick short flick moves on");
    assert_eq!(state.offset(), 100.0);
    press_and_move(
        &mut ui,
        Offset::new(10.0, 10.0),
        Offset::new(290.0, 10.0),
        6,
    );
    assert_eq!(
        state.offset(),
        200.0,
        "the offset is clamped to the outer anchors"
    );
    sleep(Duration::from_millis(150));
    ui.pointer_button(PointerButton::Primary, false);
    settle(&mut ui, 300);
    assert_eq!(state.target(), 2);
    drag(
        &mut ui,
        Offset::new(290.0, 10.0),
        Offset::new(250.0, 10.0),
        3,
    );
    settle(&mut ui, 300);
    assert_eq!(
        state.target(),
        1,
        "a flick back moves to the previous anchor"
    );
}

#[test]
fn anchored_draggable_picking_up_stops_the_settling() {
    let state = slot();
    let out = state.clone();
    let mut ui = rig(move || {
        let anchored = remember_anchored_draggable_state(vec![0.0, 200.0], 0);
        *out.borrow_mut() = Some(anchored.clone());
        stack_with(
            Modifier::new()
                .size(300.0, 50.0)
                .anchored_draggable(&anchored, Axis::Horizontal),
            || {},
        );
    });
    let state: AnchoredDraggableState = get(&state);
    press_and_move(
        &mut ui,
        Offset::new(10.0, 10.0),
        Offset::new(150.0, 10.0),
        5,
    );
    sleep(Duration::from_millis(150));
    ui.pointer_button(PointerButton::Primary, false);
    assert!(state.is_settling());
    for _ in 0..3 {
        frame(&mut ui);
    }
    let mid = state.offset();
    assert!(mid > 150.0 && mid < 200.0, "{mid}");
    press_and_move(
        &mut ui,
        Offset::new(100.0, 10.0),
        Offset::new(80.0, 10.0),
        2,
    );
    assert!(
        !state.is_settling(),
        "dragging takes over from the animation"
    );
    assert!((state.offset() - (mid - 20.0)).abs() < 0.01);
}

/// A root with a pannable, zoomable box of 200 by 200 at the top left.
fn transformable_rig() -> (Embedded, TransformableState) {
    let state = slot();
    let out = state.clone();
    let ui = rig(move || {
        let transform = remember_transformable_state();
        *out.borrow_mut() = Some(transform.clone());
        stack_with(
            Modifier::new().size(200.0, 200.0).transformable(&transform),
            || {},
        );
    });
    (ui, get(&state))
}

const TOUCH: PointerKind = PointerKind::Touch;

#[test]
fn transformable_pans_with_one_contact() {
    let (mut ui, state) = transformable_rig();
    ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Down,
        Offset::new(50.0, 50.0),
        0.5,
    );
    ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Move,
        Offset::new(70.0, 40.0),
        0.5,
    );
    ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Move,
        Offset::new(80.0, 40.0),
        0.5,
    );
    ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Up,
        Offset::new(80.0, 40.0),
        0.5,
    );
    assert_eq!(state.offset(), Offset::new(30.0, -10.0));
    assert_eq!(state.zoom(), 1.0);
    assert_eq!(state.rotation(), 0.0);
}

#[test]
fn transformable_pinches_and_rotates_with_two_contacts() {
    let (mut ui, state) = transformable_rig();
    ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Down,
        Offset::new(50.0, 100.0),
        0.5,
    );
    ui.contact(
        2,
        TOUCH,
        PointerInputPhase::Down,
        Offset::new(150.0, 100.0),
        0.5,
    );
    ui.contact(
        2,
        TOUCH,
        PointerInputPhase::Move,
        Offset::new(250.0, 100.0),
        0.5,
    );
    assert!((state.zoom() - 2.0).abs() < 1e-4, "{}", state.zoom());
    assert!(
        (state.offset().x - 50.0).abs() < 1e-3,
        "{:?}",
        state.offset()
    );
    ui.contact(
        2,
        TOUCH,
        PointerInputPhase::Move,
        Offset::new(50.0, 300.0),
        0.5,
    );
    assert!(
        (state.rotation() - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
        "{}",
        state.rotation()
    );
    let before = state.offset();
    ui.contact(
        2,
        TOUCH,
        PointerInputPhase::Up,
        Offset::new(50.0, 300.0),
        0.5,
    );
    ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Move,
        Offset::new(60.0, 100.0),
        0.5,
    );
    assert!(
        (state.offset().x - before.x - 10.0).abs() < 1e-3,
        "after one contact is lifted the other pans on its own: {:?} {:?}",
        before,
        state.offset()
    );
    assert!((state.zoom() - 2.0).abs() < 1e-4);
}

#[test]
fn transformable_zooms_with_ctrl_and_the_wheel() {
    let (mut ui, state) = transformable_rig();
    ui.pointer_moved(Offset::new(50.0, 50.0));
    assert!(
        !ui.wheel(Offset::new(0.0, 100.0)),
        "a plain wheel is not used"
    );
    ui.set_modifiers(rustle::Modifiers {
        ctrl: true,
        ..Default::default()
    });
    assert!(ui.wheel(Offset::new(0.0, 100.0)));
    assert!(state.zoom() > 1.0);
}

#[test]
fn contact_reports_whether_the_interface_used_it() {
    let (mut ui, _) = transformable_rig();
    assert!(ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Down,
        Offset::new(50.0, 50.0),
        0.5
    ));
    assert!(
        ui.contact(
            1,
            TOUCH,
            PointerInputPhase::Move,
            Offset::new(350.0, 350.0),
            0.5
        ),
        "a contact that went down on an area keeps being used outside it"
    );
    assert!(ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Up,
        Offset::new(350.0, 350.0),
        0.5
    ));
    assert!(!ui.contact(
        2,
        TOUCH,
        PointerInputPhase::Down,
        Offset::new(350.0, 350.0),
        0.5
    ));
    assert!(!ui.contact(
        2,
        TOUCH,
        PointerInputPhase::Up,
        Offset::new(350.0, 350.0),
        0.5
    ));
}

#[test]
fn a_cancelled_contact_ends_the_gesture_without_a_fling() {
    let stopped = Rc::new(Cell::new(f32::NAN));
    let out = stopped.clone();
    let mut ui = rig(move || {
        let drag = remember_draggable_state();
        let out = out.clone();
        stack_with(
            Modifier::new().size(100.0, 100.0).draggable_with(
                &drag,
                Axis::Horizontal,
                |_| {},
                move |velocity| out.set(velocity),
            ),
            || {},
        );
    });
    ui.contact(
        7,
        TOUCH,
        PointerInputPhase::Down,
        Offset::new(10.0, 10.0),
        0.5,
    );
    for x in [20.0, 40.0, 60.0] {
        sleep(Duration::from_millis(5));
        ui.contact(7, TOUCH, PointerInputPhase::Move, Offset::new(x, 10.0), 0.5);
    }
    ui.contact(
        7,
        TOUCH,
        PointerInputPhase::Cancel,
        Offset::new(60.0, 10.0),
        0.5,
    );
    assert_eq!(stopped.get(), 0.0);
}

/// Where a scroller leaves its state and position for the test to read, and what it logs.
#[derive(Clone, Default)]
struct Scroller {
    state: Slot<ScrollableState>,
    position: Slot<State<f32>>,
    log: Rc<RefCell<Vec<(&'static str, f32)>>>,
}

impl Scroller {
    /// The scroll position, which starts at 0.
    fn position(&self) -> State<f32> {
        get(&self.position)
    }

    /// The scrollable state.
    fn state(&self) -> ScrollableState {
        get(&self.state)
    }

    /// Composes a vertical scroller of `size` over `content` pixels, nested in `parent` if given,
    /// with `child` inside it, and records its state in `self`.
    fn compose(
        &self,
        parent: Option<NestedScrollConnection>,
        name: &'static str,
        size: (f32, f32),
        content: f32,
        child: impl FnOnce(),
    ) {
        let pos = remember(|| 0.0_f32);
        let max = content - size.1;
        let scroll = {
            let pos = pos.clone();
            let log = self.log.clone();
            remember_scrollable_state(move |delta| {
                let before = pos.get();
                let after = (before + delta).clamp(0.0, max);
                pos.set(after);
                log.borrow_mut().push((name, after - before));
                after - before
            })
        };
        *self.state.borrow_mut() = Some(scroll.clone());
        *self.position.borrow_mut() = Some(pos);
        let modifier = Modifier::new().size(size.0, size.1);
        let modifier = match &parent {
            Some(parent) => modifier.scrollable_nested(&scroll, Axis::Vertical, parent),
            None => modifier.scrollable(&scroll, Axis::Vertical),
        };
        stack_with(modifier, child);
    }
}

/// A rig with one scroller of 200 by 200 over `content` pixels.
fn single_scroller(content: f32) -> (Embedded, Scroller) {
    let scroller = Scroller::default();
    let handle = scroller.clone();
    let ui = rig(move || handle.compose(None, "a", (200.0, 200.0), content, || {}));
    (ui, scroller)
}

#[test]
fn scrollable_scrolls_with_the_wheel_and_clamps() {
    let (mut ui, scroller) = single_scroller(1000.0);
    let position = scroller.position();
    ui.pointer_moved(Offset::new(50.0, 50.0));
    assert!(ui.wheel(Offset::new(0.0, -120.0)));
    assert_eq!(position.get(), 120.0);
    assert!(ui.wheel(Offset::new(0.0, 30.0)));
    assert_eq!(position.get(), 90.0);
    assert!(ui.wheel(Offset::new(0.0, 500.0)));
    assert_eq!(position.get(), 0.0);
    assert!(
        !ui.wheel(Offset::new(0.0, 500.0)),
        "at the start nothing is left to use"
    );
    assert!(
        !ui.wheel(Offset::new(-40.0, 0.0)),
        "the other axis is not used"
    );
}

#[test]
fn scrollable_dragging_moves_the_content_with_the_pointer() {
    let (mut ui, scroller) = single_scroller(1000.0);
    let position = scroller.position();
    press_and_move(
        &mut ui,
        Offset::new(50.0, 150.0),
        Offset::new(50.0, 90.0),
        6,
    );
    sleep(Duration::from_millis(150));
    ui.pointer_button(PointerButton::Primary, false);
    assert!((position.get() - 60.0).abs() < 0.01, "{}", position.get());
    assert_eq!(settle(&mut ui, 5), 0, "no fling after a pause");
}

#[test]
fn scrollable_flings_and_slows_down_until_it_stops() {
    let (mut ui, scroller) = single_scroller(100_000.0);
    let (state, position) = (scroller.state(), scroller.position());
    press_and_move(
        &mut ui,
        Offset::new(50.0, 180.0),
        Offset::new(50.0, 60.0),
        8,
    );
    ui.pointer_button(PointerButton::Primary, false);
    let released = position.get();
    assert!(state.is_flinging());
    let mut steps = Vec::new();
    let mut last = released;
    for _ in 0..12 {
        frame(&mut ui);
        steps.push(position.get() - last);
        last = position.get();
    }
    assert!(steps[0] > 0.0);
    assert!(
        steps[..3].iter().cloned().fold(0.0, f32::max) > steps[11] * 1.5,
        "the fling slows down: {steps:?}"
    );
    let frames = settle(&mut ui, 2000);
    assert!(frames < 2000, "the fling comes to rest");
    assert!(!state.is_flinging());
    let rest = position.get();
    frame(&mut ui);
    assert_eq!(position.get(), rest);
    assert!(rest > released + 30.0, "{released} -> {rest}");
}

#[test]
fn touching_a_flinging_scroller_stops_it() {
    let (mut ui, scroller) = single_scroller(100_000.0);
    let (state, position) = (scroller.state(), scroller.position());
    press_and_move(
        &mut ui,
        Offset::new(50.0, 180.0),
        Offset::new(50.0, 60.0),
        8,
    );
    ui.pointer_button(PointerButton::Primary, false);
    frame(&mut ui);
    assert!(state.is_flinging());
    ui.pointer_moved(Offset::new(50.0, 100.0));
    ui.pointer_button(PointerButton::Primary, true);
    assert!(!state.is_flinging());
    let held = position.get();
    frame(&mut ui);
    frame(&mut ui);
    assert_eq!(position.get(), held);
    ui.pointer_button(PointerButton::Primary, false);
}

#[test]
fn a_fling_stopped_by_the_end_of_the_content_comes_to_rest_there() {
    let (mut ui, scroller) = single_scroller(300.0);
    let (state, position) = (scroller.state(), scroller.position());
    press_and_move(
        &mut ui,
        Offset::new(50.0, 190.0),
        Offset::new(50.0, 100.0),
        6,
    );
    ui.pointer_button(PointerButton::Primary, false);
    settle(&mut ui, 300);
    assert_eq!(position.get(), 100.0);
    assert!(!state.is_flinging());
}

/// A rig with a scroller of 200 by 200 over `outer_content` pixels that holds one of the same
/// size over `inner_content`, nested in it.
fn nested_scrollers(outer_content: f32, inner_content: f32) -> (Embedded, Scroller, Scroller) {
    let outer = Scroller::default();
    let inner = Scroller::default();
    let (o, i) = (outer.clone(), inner.clone());
    let ui = rig(move || {
        let (o, i) = (o.clone(), i.clone());
        o.clone()
            .compose(None, "outer", (200.0, 200.0), outer_content, move || {
                let connection = o.state().connection();
                i.compose(
                    Some(connection),
                    "inner",
                    (200.0, 200.0),
                    inner_content,
                    || {},
                );
            });
    });
    (ui, outer, inner)
}

#[test]
fn nested_scroll_gives_the_parent_what_the_child_cannot_use() {
    let (mut ui, outer, inner) = nested_scrollers(800.0, 400.0);
    ui.pointer_moved(Offset::new(50.0, 50.0));
    assert!(ui.wheel(Offset::new(0.0, -150.0)));
    assert_eq!(inner.position().get(), 150.0);
    assert_eq!(outer.position().get(), 0.0);
    assert!(ui.wheel(Offset::new(0.0, -150.0)));
    assert_eq!(
        inner.position().get(),
        200.0,
        "the child scrolled to its end"
    );
    assert_eq!(
        outer.position().get(),
        100.0,
        "and the parent took the rest"
    );
}

#[test]
fn nested_scroll_by_dragging_gives_the_parent_the_leftover() {
    let (mut ui, outer, inner) = nested_scrollers(800.0, 300.0);
    press_and_move(
        &mut ui,
        Offset::new(50.0, 190.0),
        Offset::new(50.0, 40.0),
        10,
    );
    sleep(Duration::from_millis(150));
    ui.pointer_button(PointerButton::Primary, false);
    assert_eq!(inner.position().get(), 100.0);
    assert!(
        (outer.position().get() - 50.0).abs() < 0.01,
        "{}",
        outer.position().get()
    );
}

#[test]
fn nested_scroll_pre_scroll_lets_the_parent_go_first() {
    let taken = Rc::new(Cell::new(0.0_f32));
    let scroller = Scroller::default();
    let (seen, handle) = (taken.clone(), scroller.clone());
    let mut ui = rig(move || {
        let seen = seen.clone();
        let connection = NestedScrollConnection::new().on_pre_scroll(move |available| {
            let share = available / 2.0;
            seen.set(seen.get() + share);
            share
        });
        handle.compose(Some(connection), "a", (200.0, 200.0), 1000.0, || {});
    });
    ui.pointer_moved(Offset::new(50.0, 50.0));
    assert!(ui.wheel(Offset::new(0.0, -100.0)));
    assert_eq!(taken.get(), 50.0);
    assert_eq!(scroller.position().get(), 50.0);
}

#[test]
fn a_fling_at_the_end_of_a_child_continues_in_the_parent() {
    let (mut ui, outer, inner) = nested_scrollers(100_000.0, 260.0);
    press_and_move(
        &mut ui,
        Offset::new(50.0, 190.0),
        Offset::new(50.0, 40.0),
        8,
    );
    ui.pointer_button(PointerButton::Primary, false);
    settle(&mut ui, 2000);
    assert_eq!(inner.position().get(), 60.0);
    assert!(
        outer.position().get() > 50.0,
        "the leftover fling scrolled the parent: {}",
        outer.position().get()
    );
}

#[test]
fn pointer_input_sees_down_move_up_with_every_pointer_and_wheel() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    let mut ui = rig(move || {
        let sink = sink.clone();
        stack_with(
            Modifier::new()
                .size(100.0, 100.0)
                .offset(20.0, 30.0)
                .pointer_input(move |event| {
                    sink.borrow_mut().push((
                        event.phase,
                        event.id,
                        event.position,
                        event.pointers.len(),
                    ));
                    event.phase == PointerInputPhase::Scroll
                }),
            || {},
        );
    });
    ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Down,
        Offset::new(40.0, 50.0),
        0.5,
    );
    ui.contact(
        2,
        TOUCH,
        PointerInputPhase::Down,
        Offset::new(60.0, 70.0),
        0.5,
    );
    ui.contact(
        2,
        TOUCH,
        PointerInputPhase::Move,
        Offset::new(300.0, 300.0),
        0.5,
    );
    ui.contact(
        2,
        TOUCH,
        PointerInputPhase::Up,
        Offset::new(300.0, 300.0),
        0.5,
    );
    ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Up,
        Offset::new(40.0, 50.0),
        0.5,
    );
    ui.pointer_moved(Offset::new(50.0, 50.0));
    assert!(ui.wheel(Offset::new(0.0, 10.0)));
    let events = events.borrow();
    use PointerInputPhase::*;
    assert_eq!(
        events.iter().map(|e| (e.0, e.1, e.3)).collect::<Vec<_>>(),
        vec![
            (Down, PointerId(1), 1),
            (Down, PointerId(2), 2),
            (Move, PointerId(2), 2),
            (Up, PointerId(2), 1),
            (Up, PointerId(1), 0),
            (Scroll, PointerId::MOUSE, 0),
        ]
    );
    assert_eq!(
        events[0].2,
        Offset::new(20.0, 20.0),
        "positions are in the area"
    );
    assert_eq!(
        events[2].2,
        Offset::new(280.0, 270.0),
        "moves outside are still delivered"
    );
}

#[test]
fn topmost_pointer_input_area_takes_the_pointer() {
    let hits = Rc::new(RefCell::new(Vec::new()));
    let sink = hits.clone();
    let mut ui = rig(move || {
        let (below, above) = (sink.clone(), sink.clone());
        stack_with(
            Modifier::new().size(200.0, 200.0).pointer_input(move |e| {
                if e.phase == PointerInputPhase::Down {
                    below.borrow_mut().push("below");
                }
                false
            }),
            move || {
                stack_with(
                    Modifier::new().size(50.0, 50.0).pointer_input(move |e| {
                        if e.phase == PointerInputPhase::Down {
                            above.borrow_mut().push("above");
                        }
                        false
                    }),
                    || {},
                );
            },
        );
    });
    ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Down,
        Offset::new(10.0, 10.0),
        0.5,
    );
    ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Up,
        Offset::new(10.0, 10.0),
        0.5,
    );
    ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Down,
        Offset::new(100.0, 100.0),
        0.5,
    );
    ui.contact(
        1,
        TOUCH,
        PointerInputPhase::Up,
        Offset::new(100.0, 100.0),
        0.5,
    );
    assert_eq!(*hits.borrow(), vec!["above", "below"]);
}

#[test]
fn double_tap_fires_on_the_second_quick_press_only() {
    let taps = Rc::new(RefCell::new(Vec::new()));
    let sink = taps.clone();
    let mut ui = rig(move || {
        let sink = sink.clone();
        stack_with(
            Modifier::new()
                .size(100.0, 100.0)
                .offset(10.0, 10.0)
                .on_double_tap(move |at| sink.borrow_mut().push(at)),
            || {},
        );
    });
    let click = |ui: &mut Embedded, at: Offset| {
        ui.pointer_moved(at);
        ui.pointer_button(PointerButton::Primary, true);
        ui.pointer_button(PointerButton::Primary, false);
    };
    click(&mut ui, Offset::new(40.0, 50.0));
    assert!(taps.borrow().is_empty());
    click(&mut ui, Offset::new(41.0, 50.0));
    assert_eq!(*taps.borrow(), vec![Offset::new(31.0, 40.0)]);
    click(&mut ui, Offset::new(41.0, 50.0));
    assert_eq!(
        taps.borrow().len(),
        1,
        "the third press is not a double tap"
    );
    sleep(Duration::from_millis(500));
    click(&mut ui, Offset::new(41.0, 50.0));
    click(&mut ui, Offset::new(100.0, 100.0));
    assert_eq!(taps.borrow().len(), 1, "far apart presses do not add up");
}

#[test]
fn double_tap_with_a_finger() {
    let taps = Rc::new(Cell::new(0));
    let sink = taps.clone();
    let mut ui = rig(move || {
        let sink = sink.clone();
        stack_with(
            Modifier::new()
                .size(100.0, 100.0)
                .on_double_tap(move |_| sink.set(sink.get() + 1)),
            || {},
        );
    });
    for _ in 0..2 {
        ui.contact(
            3,
            TOUCH,
            PointerInputPhase::Down,
            Offset::new(20.0, 20.0),
            0.5,
        );
        ui.contact(
            3,
            TOUCH,
            PointerInputPhase::Up,
            Offset::new(20.0, 20.0),
            0.5,
        );
    }
    assert_eq!(taps.get(), 1);
}

#[test]
fn secondary_button_drag_source_drops_on_a_target() {
    let dropped = Rc::new(RefCell::new(Vec::<String>::new()));
    let clicked = Rc::new(Cell::new(0));
    let (sink, clicks) = (dropped.clone(), clicked.clone());
    let mut ui = rig(move || {
        let (sink, clicks) = (sink.clone(), clicks.clone());
        row_with(Modifier::new(), move || {
            stack_with(
                Modifier::new()
                    .size(100.0, 100.0)
                    .clickable(move || clicks.set(clicks.get() + 1))
                    .drag_source_button(PointerButton::Secondary, "payload".to_string()),
                || {},
            );
            stack_with(
                Modifier::new()
                    .size(100.0, 100.0)
                    .drop_target(move |payload| {
                        if let Some(text) = payload.downcast_ref::<String>() {
                            sink.borrow_mut().push(text.clone());
                        }
                    }),
                || {},
            );
        });
    });
    ui.pointer_moved(Offset::new(50.0, 50.0));
    ui.pointer_button(PointerButton::Secondary, true);
    ui.pointer_moved(Offset::new(120.0, 50.0));
    ui.pointer_moved(Offset::new(150.0, 50.0));
    assert_eq!(ui.cursor(), rustle::CursorIcon::Grabbing);
    ui.pointer_button(PointerButton::Secondary, false);
    assert_eq!(*dropped.borrow(), vec!["payload".to_string()]);
    assert_eq!(clicked.get(), 0, "a secondary drag does not click");
    ui.pointer_moved(Offset::new(50.0, 50.0));
    ui.pointer_button(PointerButton::Secondary, true);
    ui.pointer_moved(Offset::new(52.0, 50.0));
    ui.pointer_button(PointerButton::Secondary, false);
    assert_eq!(
        dropped.borrow().len(),
        1,
        "a press that did not move is no drag"
    );
    ui.pointer_moved(Offset::new(50.0, 50.0));
    ui.pointer_button(PointerButton::Primary, true);
    ui.pointer_moved(Offset::new(150.0, 50.0));
    ui.pointer_button(PointerButton::Primary, false);
    assert_eq!(
        dropped.borrow().len(),
        1,
        "the primary button picks nothing up"
    );
}

#[test]
fn a_secondary_drag_released_outside_the_window_is_put_down() {
    let mut ui = rig(move || {
        stack_with(
            Modifier::new()
                .size(100.0, 100.0)
                .drag_source_button(PointerButton::Secondary, 1_u8),
            || {},
        );
    });
    ui.pointer_moved(Offset::new(50.0, 50.0));
    ui.pointer_button(PointerButton::Secondary, true);
    ui.pointer_moved(Offset::new(90.0, 90.0));
    assert_eq!(ui.cursor(), rustle::CursorIcon::Grabbing);
    ui.pointer_left();
    ui.pointer_button(PointerButton::Secondary, false);
    ui.pointer_moved(Offset::new(50.0, 50.0));
    assert_ne!(ui.cursor(), rustle::CursorIcon::Grabbing);
}

#[test]
fn a_gesture_state_moved_to_a_new_area_is_not_stuck_by_a_lost_release() {
    let show = Rc::new(Cell::new(true));
    let state = slot();
    let (visible, out) = (show.clone(), state.clone());
    let mut ui = rig(move || {
        let drag = remember_draggable_state();
        *out.borrow_mut() = Some(drag.clone());
        if visible.get() {
            stack_with(
                Modifier::new()
                    .size(100.0, 100.0)
                    .draggable(&drag, Axis::Horizontal),
                || {},
            );
        } else {
            stack_with(
                Modifier::new()
                    .size(120.0, 100.0)
                    .draggable(&drag, Axis::Horizontal),
                || {},
            );
        }
    });
    let state: DraggableState = get(&state);
    press_and_move(&mut ui, Offset::new(10.0, 10.0), Offset::new(50.0, 10.0), 4);
    assert!(state.is_dragging());
    show.set(false);
    ui.invalidate();
    ui.update();
    ui.pointer_button(PointerButton::Primary, false);
    ui.update();
    drag(&mut ui, Offset::new(10.0, 10.0), Offset::new(30.0, 10.0), 3);
    assert!(!state.is_dragging());
    assert!((state.offset() - 60.0).abs() < 0.01, "{}", state.offset());
}

#[test]
fn a_draggable_that_moves_with_its_own_offset_keeps_receiving_the_drag() {
    let state = slot();
    let out = state.clone();
    let mut ui = rig(move || {
        let drag = remember_draggable_state();
        *out.borrow_mut() = Some(drag.clone());
        stack_with(
            Modifier::new()
                .offset(drag.offset(), 0.0)
                .size(40.0, 40.0)
                .draggable(&drag, Axis::Horizontal),
            || {},
        );
    });
    let state: DraggableState = get(&state);
    press_and_move(
        &mut ui,
        Offset::new(20.0, 20.0),
        Offset::new(220.0, 20.0),
        10,
    );
    sleep(Duration::from_millis(150));
    ui.pointer_button(PointerButton::Primary, false);
    assert!((state.offset() - 200.0).abs() < 0.01, "{}", state.offset());
    assert!(!state.is_dragging());
    drag(
        &mut ui,
        Offset::new(220.0, 20.0),
        Offset::new(120.0, 20.0),
        5,
    );
    assert!((state.offset() - 100.0).abs() < 0.01, "{}", state.offset());
}

/// A press on a scrollbar drawn over a clickable row drags the bar alone: the row is not
/// clicked, and the state says its bar is held until the button is released.
#[test]
fn a_press_on_a_scrollbar_belongs_to_the_scrollbar() {
    let clicks = Rc::new(Cell::new(0));
    let state = ScrollState::new(0.0);
    state.set_scrollbar(ScrollbarStyle {
        visibility: ScrollbarVisibility::Always,
        ..ScrollbarStyle::default()
    });
    let (counted, shown) = (Rc::clone(&clicks), state.clone());
    let mut ui = rig(move || {
        let counted = Rc::clone(&counted);
        column_with(
            Modifier::new().size(200.0, 200.0).vertical_scroll(&shown),
            move || {
                for _ in 0..10 {
                    let counted = Rc::clone(&counted);
                    spacer(
                        Modifier::new()
                            .fill_max_width()
                            .height(100.0)
                            .clickable(move || counted.set(counted.get() + 1)),
                    );
                }
            },
        );
    });
    ui.update();
    press_and_move(
        &mut ui,
        Offset::new(195.0, 20.0),
        Offset::new(195.0, 120.0),
        4,
    );
    assert!(state.is_scrollbar_dragged());
    ui.pointer_button(PointerButton::Primary, false);
    ui.update();
    assert!(!state.is_scrollbar_dragged());
    assert!(state.offset() > 0.0);
    assert_eq!(clicks.get(), 0);
    ui.pointer_moved(Offset::new(50.0, 50.0));
    ui.pointer_button(PointerButton::Primary, true);
    ui.pointer_button(PointerButton::Primary, false);
    assert_eq!(
        clicks.get(),
        1,
        "a press beside the bar still clicks the row"
    );
}

/// A scrolling area whose state was given no style takes the `LOCAL_SCROLLBAR_STYLE` around
/// it, and one given a style with `set_scrollbar` keeps its own.
#[test]
fn a_scrollbar_takes_the_local_style_unless_given_its_own() {
    let local = ScrollbarStyle {
        thickness: 4.0,
        visibility: ScrollbarVisibility::Always,
        ..ScrollbarStyle::default()
    };
    let own = ScrollbarStyle {
        thickness: 9.0,
        ..ScrollbarStyle::default()
    };
    let (plain, styled) = (ScrollState::new(0.0), ScrollState::new(0.0));
    styled.set_scrollbar(own);
    let (shown_plain, shown_styled) = (plain.clone(), styled.clone());
    let _ui = rig(move || {
        let (plain, styled) = (shown_plain.clone(), shown_styled.clone());
        LOCAL_SCROLLBAR_STYLE.provide(local, || {
            column_with(
                Modifier::new().size(100.0, 100.0).vertical_scroll(&plain),
                || {
                    spacer(Modifier::new().size(100.0, 400.0));
                },
            );
            column_with(
                Modifier::new().size(100.0, 100.0).vertical_scroll(&styled),
                || {
                    spacer(Modifier::new().size(100.0, 400.0));
                },
            );
        });
    });
    assert_eq!(plain.scrollbar(), local);
    assert_eq!(styled.scrollbar(), own);
}

/// A draggable takes its slop from the view configuration provided around it: with a slop
/// of 30 pixels, a drag of 20 moves nothing and one of 50 moves it.
#[test]
fn draggable_takes_the_touch_slop_provided_around_it() {
    let state = slot();
    let out = state.clone();
    let configuration = rustle::ViewConfiguration {
        touch_slop: 30.0,
        ..rustle::ViewConfiguration::default()
    };
    let mut ui = rig(move || {
        rustle::LOCAL_VIEW_CONFIGURATION.provide(configuration, || {
            let drag = remember_draggable_state();
            *out.borrow_mut() = Some(drag.clone());
            stack_with(
                Modifier::new()
                    .size(100.0, 100.0)
                    .draggable(&drag, Axis::Vertical),
                || {},
            );
        });
    });
    let state: DraggableState = get(&state);
    drag(&mut ui, Offset::new(50.0, 10.0), Offset::new(50.0, 30.0), 2);
    assert_eq!(state.offset(), 0.0);
    drag(&mut ui, Offset::new(50.0, 10.0), Offset::new(50.0, 60.0), 5);
    assert!(state.offset() > 0.0, "{}", state.offset());
}

/// Presses are counted into double clicks by the double-tap timeout of the view
/// configuration of the area pressed: with none, a second press at once is a first again.
#[test]
fn presses_count_by_the_double_tap_timeout_provided_around_them() {
    for timeout in [300, 0] {
        let counts = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&counts);
        let configuration = rustle::ViewConfiguration {
            double_tap_timeout_millis: timeout,
            ..rustle::ViewConfiguration::default()
        };
        let mut ui = rig(move || {
            let seen = Rc::clone(&seen);
            rustle::LOCAL_VIEW_CONFIGURATION.provide(configuration, || {
                stack_with(
                    Modifier::new()
                        .size(100.0, 100.0)
                        .on_press(move |press| seen.borrow_mut().push(press.click_count)),
                    || {},
                );
            });
        });
        for _ in 0..2 {
            ui.pointer_moved(Offset::new(50.0, 50.0));
            ui.pointer_button(PointerButton::Primary, true);
            ui.pointer_button(PointerButton::Primary, false);
        }
        let expected = if timeout > 0 { vec![1, 2] } else { vec![1, 1] };
        assert_eq!(*counts.borrow(), expected);
    }
}

/// A handle drawn over the edge of a scrolling area takes a press on it: the handle is
/// dragged and the scrollbar beneath it is left alone.
#[test]
fn a_handle_over_a_scrollbar_takes_the_press() {
    let state = ScrollState::new(0.0);
    state.set_scrollbar(ScrollbarStyle {
        visibility: ScrollbarVisibility::Always,
        ..ScrollbarStyle::default()
    });
    let held = slot();
    let (shown, out) = (state.clone(), held.clone());
    let mut ui = rig(move || {
        let handle = remember_draggable_state();
        *out.borrow_mut() = Some(handle.clone());
        let shown = shown.clone();
        stack_with(Modifier::new().size(200.0, 200.0), move || {
            column_with(
                Modifier::new().size(200.0, 200.0).vertical_scroll(&shown),
                || spacer(Modifier::new().fill_max_width().height(1000.0)),
            );
            stack_with(
                Modifier::new()
                    .offset(190.0, 0.0)
                    .size(10.0, 200.0)
                    .draggable(&handle, Axis::Horizontal),
                || {},
            );
        });
    });
    ui.update();
    let handle: DraggableState = get(&held);
    press_and_move(
        &mut ui,
        Offset::new(195.0, 20.0),
        Offset::new(150.0, 120.0),
        6,
    );
    assert!(handle.is_dragging());
    assert!(!state.is_scrollbar_dragged());
    assert_eq!(state.offset(), 0.0);
    ui.pointer_button(PointerButton::Primary, false);
}
