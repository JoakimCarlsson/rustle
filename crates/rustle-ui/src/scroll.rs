//! Scroll state shared between a scrolling modifier and the code driving it,
//! and the overlay scrollbar that goes with it.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rustle_graphics::Color;
use rustle_runtime::{FrameClock, State, frame_clock, group, remember};

use crate::gesture::{NestedScrollConnection, ScrollableCore, ScrollableState};
use crate::semantics::{Role, ScrollDirection, Semantics};

/// How long a scrollbar stays fully visible after the last scroll.
const HOLD: Duration = Duration::from_millis(900);
/// How long it then takes to fade out.
const FADE: Duration = Duration::from_millis(350);

/// When a scrolling area shows its scrollbar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScrollbarVisibility {
    /// Appears while scrolling or while the pointer is over it, then fades out.
    #[default]
    Fade,
    /// Always shown, as long as the content is longer than the area.
    Always,
    /// Never shown, and not draggable.
    Hidden,
}

/// How the overlay scrollbar of a scrolling area looks.
///
/// The bar is drawn over the content at the end of the scrolling axis, so it
/// takes no space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarStyle {
    pub visibility: ScrollbarVisibility,
    /// The color of the thumb while at rest. Its alpha is also the most
    /// opaque the thumb gets when not hovered or dragged.
    pub color: Color,
    /// The color of the thumb while the pointer is over it or it is dragged.
    pub active_color: Color,
    /// The thickness of the thumb, in logical pixels.
    pub thickness: f32,
    /// The thickness of the strip that reacts to the pointer, at least as
    /// much as `thickness`.
    pub hit_thickness: f32,
    /// The shortest the thumb gets, however long the content.
    pub min_length: f32,
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        Self {
            visibility: ScrollbarVisibility::Fade,
            // A mid-gray reads on light and dark backgrounds alike.
            color: Color::rgba(128, 128, 128, 140),
            active_color: Color::rgba(128, 128, 128, 210),
            thickness: 6.0,
            hit_thickness: 14.0,
            min_length: 28.0,
        }
    }
}

/// What a scrolling area remembers about its scrollbar.
struct Bar {
    style: Cell<ScrollbarStyle>,
    last_active: Cell<Option<Instant>>,
    /// The pointer is over the scrollbar's strip.
    hovering: Cell<bool>,
    /// Where on the thumb a drag took hold, from the thumb's start.
    grab: Cell<Option<f32>>,
    /// A frame callback is running to redraw while the bar fades.
    ticking: Cell<bool>,
    /// Changed on every frame of a fade, so the area is laid out again.
    tick: State<u32>,
    clock: FrameClock,
    /// Fling and drag state shared with the [`ScrollableState`] this area hands out.
    core: Rc<ScrollableCore>,
}

/// How far a scrolling area is scrolled, and how far it can go.
///
/// The offset is observable state, so changing it recomposes, which lets
/// content depend on the scroll position. The extent is filled in by layout.
#[derive(Clone)]
pub struct ScrollState {
    offset: State<f32>,
    /// Content length and viewport length along the scrolling axis.
    extent: State<(f32, f32)>,
    bar: Rc<Bar>,
}

impl ScrollState {
    /// Distance the content is scrolled, from 0 (at the start).
    pub fn offset(&self) -> f32 {
        self.offset.get()
    }

    /// The largest offset: how much of the content lies beyond the viewport.
    pub fn max_offset(&self) -> f32 {
        let (content, viewport) = self.extent.get();
        (content - viewport).max(0.0)
    }

    /// The length of the visible part of the content, as of the latest layout.
    pub fn viewport_length(&self) -> f32 {
        self.extent.get().1
    }

    /// The length of all the content, as of the latest layout.
    pub fn content_length(&self) -> f32 {
        self.extent.get().0
    }

    /// Whether the content is scrolled to its end, give or take a pixel.
    pub fn at_end(&self) -> bool {
        self.offset() >= self.max_offset() - 1.0
    }

    /// Scrolls by `delta`, stopping at either end, and shows the scrollbar.
    pub fn scroll_by(&self, delta: f32) {
        self.scroll_to(self.offset() + delta);
    }

    /// Scrolls to `offset`, stopping at either end, and shows the scrollbar.
    pub fn scroll_to(&self, offset: f32) {
        let offset = offset.clamp(0.0, self.max_offset());
        if offset != self.offset() {
            self.offset.set(offset);
            self.show_scrollbar();
        }
    }

    /// A description of this area for assistive technology: a group that scrolls by most
    /// of a viewport when asked to move along its axis (`vertical` or horizontal).
    pub fn semantics(&self, vertical: bool) -> Semantics {
        let this = self.clone();
        Semantics::new(Role::Group).on_scroll(move |direction| {
            let page = (this.viewport_length() * 0.8).max(1.0);
            match (vertical, direction) {
                (true, ScrollDirection::Up) | (false, ScrollDirection::Left) => {
                    this.scroll_by(-page)
                }
                (true, ScrollDirection::Down) | (false, ScrollDirection::Right) => {
                    this.scroll_by(page)
                }
                _ => {}
            }
        })
    }

    /// Sets the offset without limiting it to the content measured so far and
    /// without showing the scrollbar. For content that is about to change
    /// length, such as a list that learns the height of its items; layout
    /// still keeps what is drawn inside the content.
    pub fn jump_to(&self, offset: f32) {
        let offset = offset.max(0.0);
        if offset != self.offset() {
            self.offset.set(offset);
        }
    }

    /// This area as a [`ScrollableState`], for [`Modifier::scrollable`](crate::Modifier::scrollable)
    /// to scroll it by drag, wheel and fling. Every call returns a handle to the same fling and drag.
    pub fn scrollable(&self) -> ScrollableState {
        let this = self.clone();
        ScrollableState::from_parts(
            Rc::new(move |delta| {
                let before = this.offset();
                this.scroll_by(delta);
                this.offset() - before
            }),
            Rc::clone(&self.bar.core),
        )
    }

    /// Keeps scrolling at `velocity` logical pixels per second (positive towards the end),
    /// slowing down until it stops or reaches an end.
    pub fn fling(&self, velocity: f32) {
        self.scrollable().fling(velocity, None);
    }

    /// Stops a fling that is running.
    pub fn stop_fling(&self) {
        self.scrollable().stop_fling();
    }

    /// A connection that gives scroll and fling its children leave over to this area. See
    /// [`ScrollableState::connection`].
    pub fn connection(&self) -> NestedScrollConnection {
        self.scrollable().connection()
    }

    /// Changes how the scrollbar looks and when it shows.
    pub fn set_scrollbar(&self, style: ScrollbarStyle) {
        self.bar.style.set(style);
    }

    /// How the scrollbar looks and when it shows.
    pub fn scrollbar(&self) -> ScrollbarStyle {
        self.bar.style.get()
    }

    /// Shows the scrollbar now, and lets it fade out after a moment.
    pub fn show_scrollbar(&self) {
        self.bar.last_active.set(Some(Instant::now()));
        if !self.bar.ticking.replace(true) {
            self.schedule_fade();
        }
    }

    /// Asks for frames until the scrollbar has faded, so it is redrawn as it does.
    fn schedule_fade(&self) {
        let this = self.clone();
        self.bar.clock.request_frame(move |_| {
            this.bar.tick.update(|tick| *tick = tick.wrapping_add(1));
            if this.fade_pending() {
                this.schedule_fade();
            } else {
                this.bar.ticking.set(false);
            }
        });
    }

    /// Whether the scrollbar is still changing on its own.
    fn fade_pending(&self) -> bool {
        !self.bar.hovering.get()
            && self.bar.grab.get().is_none()
            && self
                .bar
                .last_active
                .get()
                .is_some_and(|at| at.elapsed() < HOLD + FADE)
    }

    /// How opaque the scrollbar is right now, from 0 to 1.
    pub(crate) fn scrollbar_alpha(&self) -> f32 {
        match self.bar.style.get().visibility {
            ScrollbarVisibility::Hidden => 0.0,
            ScrollbarVisibility::Always => 1.0,
            ScrollbarVisibility::Fade => {
                if self.bar.hovering.get() || self.bar.grab.get().is_some() {
                    return 1.0;
                }
                self.bar
                    .last_active
                    .get()
                    .map_or(0.0, |at| fade_alpha(at.elapsed()))
            }
        }
    }

    /// Whether the pointer is over the scrollbar or dragging it.
    pub(crate) fn scrollbar_active(&self) -> bool {
        self.bar.hovering.get() || self.bar.grab.get().is_some()
    }

    /// Records that the pointer entered or left the scrollbar's strip.
    pub(crate) fn set_scrollbar_hovered(&self, hovering: bool) {
        if self.bar.hovering.replace(hovering) != hovering {
            self.show_scrollbar();
        }
    }

    /// Handles the pointer at `position` along a track `track` long; the first
    /// call of a drag is the press. A press on the thumb grabs it where it was touched; a
    /// press elsewhere on the track centers the thumb under the pointer.
    pub(crate) fn drag_thumb(&self, position: f32, track: f32) {
        let first = self.bar.grab.get().is_none();
        let (content, viewport) = self.extent.get();
        let min = self.bar.style.get().min_length;
        let (start, length) = thumb(self.offset(), content, viewport, track, min);
        if first {
            let grab = if position >= start && position <= start + length {
                position - start
            } else {
                length / 2.0
            };
            self.bar.grab.set(Some(grab));
        }
        let grab = self.bar.grab.get().unwrap_or(length / 2.0);
        self.scroll_to(offset_for_thumb(
            position - grab,
            content,
            viewport,
            track,
            min,
        ));
    }

    /// Ends a drag of the thumb.
    pub(crate) fn end_thumb_drag(&self) {
        self.bar.grab.set(None);
        self.show_scrollbar();
    }

    /// Records the lengths layout found. Changing them recomposes, so content
    /// that depends on the viewport, such as a lazy list, catches up.
    pub(crate) fn set_extent(&self, content: f32, viewport: f32) {
        if self.extent.get() != (content, viewport) {
            self.extent.set((content, viewport));
        }
    }
}

/// How opaque a fading scrollbar is `elapsed` after the last scroll.
fn fade_alpha(elapsed: Duration) -> f32 {
    if elapsed <= HOLD {
        1.0
    } else {
        (1.0 - (elapsed - HOLD).as_secs_f32() / FADE.as_secs_f32()).clamp(0.0, 1.0)
    }
}

/// Where the thumb of a scrollbar is on a track `track` long: its start and
/// its length, for content `content` long seen through `viewport`, scrolled to
/// `offset`. The thumb is never shorter than `min` (or the track).
pub(crate) fn thumb(offset: f32, content: f32, viewport: f32, track: f32, min: f32) -> (f32, f32) {
    if content <= 0.0 || content <= viewport {
        return (0.0, track);
    }
    let length = (track * viewport / content).clamp(min.min(track), track);
    let max = content - viewport;
    let start = (track - length) * (offset / max).clamp(0.0, 1.0);
    (start, length)
}

/// The offset that puts the thumb's start at `start`; the inverse of [`thumb`].
pub(crate) fn offset_for_thumb(
    start: f32,
    content: f32,
    viewport: f32,
    track: f32,
    min: f32,
) -> f32 {
    let (_, length) = thumb(0.0, content, viewport, track, min);
    let room = track - length;
    if room <= 0.0 {
        return 0.0;
    }
    (start / room).clamp(0.0, 1.0) * (content - viewport).max(0.0)
}

/// Remembers a [`ScrollState`] at this call site.
#[track_caller]
pub fn remember_scroll_state() -> ScrollState {
    group(|| {
        let offset = remember(|| 0.0);
        let extent = remember(|| (0.0, 0.0));
        let tick = remember(|| 0_u32);
        let bar = remember(|| {
            Rc::new(Bar {
                style: Cell::new(ScrollbarStyle::default()),
                last_active: Cell::new(None),
                hovering: Cell::new(false),
                grab: Cell::new(None),
                ticking: Cell::new(false),
                tick,
                clock: frame_clock(),
                core: Rc::new(ScrollableCore::new()),
            })
        });
        ScrollState {
            offset,
            extent,
            bar: bar.get(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_covers_the_track_when_everything_fits() {
        assert_eq!(thumb(0.0, 100.0, 200.0, 180.0, 20.0), (0.0, 180.0));
    }

    #[test]
    fn thumb_is_proportional_and_moves_with_the_offset() {
        let (start, length) = thumb(0.0, 1000.0, 250.0, 200.0, 20.0);
        assert_eq!((start, length), (0.0, 50.0));
        let (start, length) = thumb(750.0, 1000.0, 250.0, 200.0, 20.0);
        assert_eq!((start, length), (150.0, 50.0));
    }

    #[test]
    fn thumb_keeps_a_minimum_length() {
        let (_, length) = thumb(0.0, 100_000.0, 250.0, 200.0, 28.0);
        assert_eq!(length, 28.0);
    }

    #[test]
    fn dragging_the_thumb_inverts_its_position() {
        for offset in [0.0, 120.0, 500.0, 750.0] {
            let (start, _) = thumb(offset, 1000.0, 250.0, 200.0, 20.0);
            let back = offset_for_thumb(start, 1000.0, 250.0, 200.0, 20.0);
            assert!((back - offset).abs() < 0.01, "{offset} came back as {back}");
        }
    }

    #[test]
    fn scrollbar_fades_after_a_hold() {
        assert_eq!(fade_alpha(Duration::ZERO), 1.0);
        assert_eq!(fade_alpha(HOLD), 1.0);
        let half = fade_alpha(HOLD + FADE / 2);
        assert!((half - 0.5).abs() < 0.01);
        assert_eq!(fade_alpha(HOLD + FADE * 2), 0.0);
    }

    fn state() -> ScrollState {
        let slot = Rc::new(std::cell::RefCell::new(None));
        let remembered = Rc::clone(&slot);
        let mut composition = crate::composition(move || {
            *remembered.borrow_mut() = Some(remember_scroll_state());
        });
        composition.compose();
        let state = slot.borrow().clone().expect("composed");
        // 1000 of content seen through 250, so 750 can be scrolled.
        state.set_extent(1000.0, 250.0);
        state
    }

    #[test]
    fn dragging_the_thumb_scrolls_by_the_distance_it_moves() {
        let state = state();
        // The thumb is 50 long on a track of 200; grab it 10 from its start.
        state.drag_thumb(10.0, 200.0);
        assert_eq!(state.offset(), 0.0);
        state.drag_thumb(110.0, 200.0);
        assert_eq!(state.offset(), 500.0);
        state.drag_thumb(500.0, 200.0);
        assert_eq!(state.offset(), 750.0);
        state.end_thumb_drag();
        // Pressing the track away from the thumb centers the thumb there.
        state.drag_thumb(125.0, 200.0);
        assert_eq!(state.offset(), 500.0);
    }

    #[test]
    fn jumping_is_not_limited_to_the_measured_content() {
        let state = state();
        state.scroll_to(5000.0);
        assert_eq!(state.offset(), 750.0);
        state.jump_to(5000.0);
        assert_eq!(state.offset(), 5000.0);
    }

    /// Flings at `velocity` over frames `frame_ms` apart until it rests and returns the offset
    /// it stopped at, each time on content long enough not to stop it.
    fn flung_distance(velocity: f32, frame_ms: u64) -> f32 {
        let slot = Rc::new(std::cell::RefCell::new(None));
        let remembered = Rc::clone(&slot);
        let mut composition = crate::composition(move || {
            *remembered.borrow_mut() = Some(remember_scroll_state());
        });
        composition.compose();
        let state = slot.borrow().clone().expect("composed");
        state.set_extent(1_000_000.0, 100.0);
        state.fling(velocity);
        let mut now = Duration::ZERO;
        for _ in 0..10_000 {
            if !composition.has_pending_frame() {
                break;
            }
            now += Duration::from_millis(frame_ms);
            composition.run_frame(now);
            composition.recompose();
        }
        assert!(!state.scrollable().is_flinging());
        state.offset()
    }

    #[test]
    fn a_fling_travels_the_same_distance_at_any_frame_rate() {
        let expected = 3000.0 / 3.0;
        for frame_ms in [4, 8, 16, 33, 50] {
            let distance = flung_distance(3000.0, frame_ms);
            let rest = 20.0 / 3.0;
            assert!(
                (distance - (expected - rest / 2.0)).abs() < rest + expected * 0.01,
                "{frame_ms} ms frames: {distance} is not near {expected}"
            );
        }
        let backwards = flung_distance(-3000.0, 16);
        assert_eq!(backwards, 0.0);
    }
}
