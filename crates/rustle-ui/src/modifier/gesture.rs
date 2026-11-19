use std::rc::Rc;

use rustle_geometry::Offset;

use super::{Element, Modifier};
use crate::gesture::{
    AnchoredDraggableState, Axis, DraggableState, NestedScrollConnection, PointerInput,
    PointerInputPhase, ScrollableState, TransformableState,
};
use crate::input::PointerButton;

impl Modifier {
    /// Sees the raw stream of pointer events on the area: each pointer that goes down on it
    /// (the mouse's primary button, or a finger or pen contact), moves, is lifted or is
    /// cancelled, and each wheel turn. Several pointers can be down at once; every event lists
    /// all of them in [`PointerInput::pointers`]. Once a pointer went down on the area, its
    /// later events are delivered here wherever it moves.
    ///
    /// `on_event` returns whether it used the event. Only wheel events care: an unused wheel
    /// event goes on to the scrolling areas. The topmost area under a pointer that goes down
    /// receives it.
    pub fn pointer_input(self, on_event: impl Fn(&PointerInput) -> bool + 'static) -> Self {
        self.then(Element::PointerInput(Rc::new(on_event)))
    }

    /// Lets the area be dragged along `axis`: `state` records how far (see
    /// [`DraggableState::offset`]), how fast it was let go and whether a drag is going on.
    /// A drag starts once the pointer moved the
    /// [`touch_slop`](crate::ViewConfiguration::touch_slop) of the
    /// [`LOCAL_VIEW_CONFIGURATION`](crate::LOCAL_VIEW_CONFIGURATION) along the axis, as an
    /// anchored drag and a drag that scrolls do. Use the offset to place the content, for instance with [`offset`](Self::offset).
    pub fn draggable(self, state: &DraggableState, axis: Axis) -> Self {
        self.draggable_inner(state, axis, true, Rc::new(|_| {}), Rc::new(|_| {}))
    }

    /// Like [`draggable`](Self::draggable), but leaves the offset of `state` alone and calls
    /// `on_delta` with each distance dragged along the axis and `on_stopped` with the speed
    /// (pixels per second along the axis) the pointer was let go at.
    pub fn draggable_with(
        self,
        state: &DraggableState,
        axis: Axis,
        on_delta: impl Fn(f32) + 'static,
        on_stopped: impl Fn(f32) + 'static,
    ) -> Self {
        self.draggable_inner(state, axis, false, Rc::new(on_delta), Rc::new(on_stopped))
    }

    /// The pointer-input handler behind both draggable forms.
    fn draggable_inner(
        self,
        state: &DraggableState,
        axis: Axis,
        auto_apply: bool,
        on_delta: Rc<dyn Fn(f32)>,
        on_stopped: Rc<dyn Fn(f32)>,
    ) -> Self {
        let state = state.clone();
        let touch_slop = crate::view_configuration::current_view_configuration().touch_slop;
        self.pointer_input(move |event| {
            let recognizer = Rc::clone(&state.recognizer);
            state.hooks_apply(auto_apply, &*on_delta, &*on_stopped, |hooks| {
                recognizer.handle(event, axis, touch_slop, hooks)
            });
            false
        })
    }

    /// Lets the area be dragged along `axis` and settle on the anchors of `state` when let
    /// go: the nearest, or the next one in the direction of a fling. Place the content at
    /// [`AnchoredDraggableState::offset`].
    pub fn anchored_draggable(self, state: &AnchoredDraggableState, axis: Axis) -> Self {
        let state = state.clone();
        let touch_slop = crate::view_configuration::current_view_configuration().touch_slop;
        self.pointer_input(move |event| {
            let recognizer = state.recognizer();
            let started = || state.set_dragging(true);
            let delta = |delta: f32| state.drag_by(delta);
            let stopped = |velocity: f32| {
                state.set_dragging(false);
                state.settle(velocity);
            };
            recognizer.handle(
                event,
                axis,
                touch_slop,
                &crate::gesture::drag_hooks(&|| {}, &started, &delta, &stopped),
            );
            false
        })
    }

    /// Pans, zooms and rotates with one or more pointers on the area, recording the result in
    /// `state`: one pointer pans, two or more also zoom and rotate about their midpoint.
    /// Ctrl and the wheel zoom.
    pub fn transformable(self, state: &TransformableState) -> Self {
        let target = state.clone();
        self.transformable_with(state, move |zoom, pan, rotation| {
            target.transform_by(zoom, pan, rotation);
        })
    }

    /// Like [`transformable`](Self::transformable), calling `on_transform` with the change
    /// in scale (a factor), pan (logical pixels in the window) and rotation (radians) instead
    /// of applying it to `state`.
    pub fn transformable_with(
        self,
        state: &TransformableState,
        on_transform: impl Fn(f32, Offset, f32) + 'static,
    ) -> Self {
        let state = state.clone();
        self.pointer_input(move |event| state.handle(event, &on_transform))
    }

    /// Scrolls `state` along `axis` with the wheel, by dragging and by flinging after a fast
    /// drag. Dragging the pointer one way scrolls the content that way.
    ///
    /// Wheel events go to this area before the areas made with [`scroll`](Self::scroll), and
    /// are used when `state` scrolled.
    pub fn scrollable(self, state: &ScrollableState, axis: Axis) -> Self {
        self.scrollable_inner(state, axis, None)
    }

    /// Like [`scrollable`](Self::scrollable), nested in a parent that scrolls too: `parent`
    /// takes a share of each scroll before this area and what this area leaves over after
    /// it, and what is left of a fling when this area hits its end. A parent that is a
    /// [`ScrollableState`] offers [`ScrollableState::connection`].
    pub fn scrollable_nested(
        self,
        state: &ScrollableState,
        axis: Axis,
        parent: &NestedScrollConnection,
    ) -> Self {
        self.scrollable_inner(state, axis, Some(parent.clone()))
    }

    /// The pointer-input handler behind both scrollable forms.
    fn scrollable_inner(
        self,
        state: &ScrollableState,
        axis: Axis,
        parent: Option<NestedScrollConnection>,
    ) -> Self {
        let state = state.clone();
        let touch_slop = crate::view_configuration::current_view_configuration().touch_slop;
        self.pointer_input(move |event| {
            let parent = parent.clone();
            if event.phase == PointerInputPhase::Scroll {
                let amount = axis.of(event.scroll_delta);
                if amount == 0.0 {
                    return false;
                }
                state.stop_fling();
                return dispatch_scroll(&state, parent.as_ref(), -amount) != 0.0;
            }
            let pressed = || state.stop_fling();
            let started = || state.set_scrolling(true);
            let delta = |delta: f32| {
                dispatch_scroll(&state, parent.as_ref(), -delta);
            };
            let stopped = |velocity: f32| {
                state.set_scrolling(false);
                let velocity = -velocity;
                let taken = parent.as_ref().map_or(0.0, |p| p.pre_fling(velocity));
                state.fling(velocity - taken, parent.clone());
            };
            state.core.recognizer.handle(
                event,
                axis,
                touch_slop,
                &crate::gesture::drag_hooks(&pressed, &started, &delta, &stopped),
            )
        })
    }

    /// Runs `on_double_tap` for a double click with the primary button or a double tap, at
    /// the position of the second press.
    pub fn on_double_tap(self, on_double_tap: impl Fn(Offset) + 'static) -> Self {
        self.on_press(move |press| {
            if press.click_count == 2 && press.button == PointerButton::Primary {
                on_double_tap(press.position);
            }
        })
    }
}

/// Scrolls by `delta`, offering it to `parent` before and after. Returns everything that
/// was consumed by the parent's pre-scroll, `state` and the parent's post-scroll together.
fn dispatch_scroll(
    state: &ScrollableState,
    parent: Option<&NestedScrollConnection>,
    delta: f32,
) -> f32 {
    let before = parent.map_or(0.0, |parent| parent.pre_scroll(delta));
    let available = delta - before;
    let consumed = state.scroll_by(available);
    let after = parent.map_or(0.0, |parent| {
        parent.post_scroll(consumed, available - consumed)
    });
    before + consumed + after
}
