//! Scroll state shared between a scrolling modifier and the code driving it,
//! and the overlay scrollbar that goes with it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use rustle_graphics::Color;
use rustle_runtime::{
    FrameClock, State, frame_clock, group, is_composing, mutable_state, remember,
};

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
    /// When it shows.
    pub visibility: ScrollbarVisibility,
    /// The color of the thumb while the pointer is not over it, Compose's `unhoverColor`.
    /// Its alpha is also the most opaque a fading thumb gets.
    pub color: Color,
    /// The color of the thumb while the pointer is over it or it is dragged, Compose's
    /// `hoverColor`.
    pub active_color: Color,
    /// The thickness of the thumb, in logical pixels.
    pub thickness: f32,
    /// The thickness of the strip that reacts to the pointer, at least as
    /// much as `thickness`.
    pub hit_thickness: f32,
    /// The shortest the thumb gets, however long the content, Compose's `minimalHeight`.
    pub min_length: f32,
    /// The radius of the thumb's corners, in logical pixels, Compose's `shape`.
    pub corner_radius: f32,
    /// How long the thumb takes to change between its two colors when the pointer comes
    /// onto it or leaves it, Compose's `hoverDurationMillis`.
    pub hover_duration: Duration,
}

/// The look of the scrollbars of scrolling areas composed below it, as Compose Desktop's
/// `LocalScrollbarStyle`: an area whose [`ScrollState`] was not given a style of its own with
/// [`ScrollState::set_scrollbar`] takes the one in effect where it is shown with
/// [`Modifier::vertical_scroll`](crate::Modifier::vertical_scroll) or
/// [`Modifier::horizontal_scroll`](crate::Modifier::horizontal_scroll). A design system
/// provides it once at its root.
pub static LOCAL_SCROLLBAR_STYLE: rustle_runtime::CompositionLocal<ScrollbarStyle> =
    rustle_runtime::CompositionLocal::new(ScrollbarStyle::default);

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
            corner_radius: 3.0,
            hover_duration: Duration::from_millis(300),
        }
    }
}

impl Bar {
    /// A bar at rest, redrawn through `tick`, fading on `clock` once it has one.
    fn new(tick: State<u32>, clock: Option<FrameClock>) -> Self {
        Self {
            style: Cell::new(ScrollbarStyle::default()),
            styled: Cell::new(false),
            last_active: Cell::new(None),
            hovering: Cell::new(false),
            grab: Cell::new(None),
            reverse: Cell::new(false),
            hover_changed: Cell::new(None),
            ticking: Cell::new(false),
            tick,
            clock: RefCell::new(clock),
            core: Rc::new(ScrollableCore::new()),
        }
    }
}

/// What a scrolling area remembers about its scrollbar.
struct Bar {
    style: Cell<ScrollbarStyle>,
    /// The style was given with [`ScrollState::set_scrollbar`], so the
    /// [`LOCAL_SCROLLBAR_STYLE`] around the area does not replace it.
    styled: Cell<bool>,
    last_active: Cell<Option<Instant>>,
    /// The pointer is over the scrollbar's strip.
    hovering: Cell<bool>,
    /// Where on the thumb a drag took hold, from the thumb's start.
    grab: Cell<Option<f32>>,
    /// Whether the offset counts from the end of the content, as an area shown with
    /// `reverse_scrolling` counts it.
    reverse: Cell<bool>,
    /// When the pointer last came onto the strip or left it, for the change of color.
    hover_changed: Cell<Option<Instant>>,
    /// A frame callback is running to redraw while the bar fades.
    ticking: Cell<bool>,
    /// Changed on every frame of a fade, so the area is laid out again.
    tick: State<u32>,
    /// The frame clock of the composition that shows the area, once one has.
    clock: RefCell<Option<FrameClock>>,
    /// Fling and drag state shared with the [`ScrollableState`] this area hands out.
    core: Rc<ScrollableCore>,
}

/// How far a scrolling area is scrolled, and how far it can go.
///
/// The offset is observable state, so changing it recomposes, which lets
/// content depend on the scroll position. The extent is filled in by layout.
///
/// Like Compose's `ScrollState(initial)`, it can be made outside composition with
/// [`ScrollState::new`], by a state holder that owns it, or remembered at a call site
/// with [`remember_scroll_state`]. Two handles are equal when they are the same state.
#[derive(Clone)]
pub struct ScrollState {
    offset: State<f32>,
    /// Content length and viewport length along the scrolling axis.
    extent: State<(f32, f32)>,
    bar: Rc<Bar>,
}

impl PartialEq for ScrollState {
    /// Whether both handles are the same state.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.bar, &other.bar)
    }
}

impl std::fmt::Debug for ScrollState {
    /// The offset and the extent.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScrollState")
            .field("offset", &self.offset.get())
            .field("extent", &self.extent.get())
            .finish()
    }
}

impl Default for ScrollState {
    /// A state scrolled to the start.
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl ScrollState {
    /// A state scrolled `initial` logical pixels from the start, which belongs to no
    /// composition: Compose's `ScrollState(initial)`. The area that shows it, through
    /// [`Modifier::vertical_scroll`](crate::Modifier::vertical_scroll) and the like,
    /// keeps `initial` until its content is measured.
    pub fn new(initial: f32) -> Self {
        Self {
            offset: mutable_state(initial.max(0.0)),
            extent: mutable_state((0.0, 0.0)),
            bar: Rc::new(Bar::new(mutable_state(0_u32), None)),
        }
    }

    /// Whether the offset counts from the end of the content: the area that shows the
    /// state was given `reverse_scrolling`, so an offset of 0 shows the end and scrolling
    /// towards the start makes it larger, as Compose's `reverseScrolling` does.
    pub fn reverse_scrolling(&self) -> bool {
        self.bar.reverse.get()
    }

    /// Records whether the area that shows the state counts its offset from the end.
    pub(crate) fn set_reverse_scrolling(&self, reverse: bool) {
        self.bar.reverse.set(reverse);
    }

    /// How far the content is moved up (or left) from its start as drawn: the offset, or,
    /// counted from the end, what is left of the room once it is taken away.
    pub(crate) fn drawn_offset(&self) -> f32 {
        let offset = self.offset().clamp(0.0, self.max_offset());
        if self.reverse_scrolling() {
            self.max_offset() - offset
        } else {
            offset
        }
    }

    /// Scrolls by `delta` towards the end of the content as drawn, as the wheel does: the
    /// offset grows, or, counted from the end, shrinks.
    pub fn scroll_drawn_by(&self, delta: f32) {
        if self.reverse_scrolling() {
            self.scroll_by(-delta);
        } else {
            self.scroll_by(delta);
        }
    }

    /// Gives the state the frame clock of the composition being composed, if it has
    /// none yet, so its scrollbar can fade there.
    pub(crate) fn attach_clock(&self) {
        if self.bar.clock.borrow().is_none() && is_composing() {
            *self.bar.clock.borrow_mut() = Some(frame_clock());
        }
        self.bar.core.attach_clock();
    }

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
                    this.scroll_drawn_by(-page)
                }
                (true, ScrollDirection::Down) | (false, ScrollDirection::Right) => {
                    this.scroll_drawn_by(page)
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

    /// Changes how the scrollbar looks and when it shows. A style given here wins over the
    /// [`LOCAL_SCROLLBAR_STYLE`] of the composition that shows the area.
    pub fn set_scrollbar(&self, style: ScrollbarStyle) {
        self.bar.style.set(style);
        self.bar.styled.set(true);
    }

    /// Takes the [`LOCAL_SCROLLBAR_STYLE`] in effect as the scrollbar's look, while composing
    /// and unless a style was given with [`set_scrollbar`](Self::set_scrollbar).
    pub(crate) fn follow_local_scrollbar(&self) {
        if !self.bar.styled.get() && rustle_runtime::is_composing() {
            self.bar.style.set(LOCAL_SCROLLBAR_STYLE.current());
        }
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
        let clock = self.bar.clock.borrow().clone();
        let Some(clock) = clock else {
            self.bar.ticking.set(false);
            return;
        };
        clock.request_frame(move |_| {
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
        let recoloring = self
            .bar
            .hover_changed
            .get()
            .is_some_and(|at| at.elapsed() < self.bar.style.get().hover_duration);
        recoloring
            || !self.bar.hovering.get()
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

    /// Whether the pointer holds the scrollbar's thumb, as Compose's
    /// `interactionSource.collectIsDraggedAsState()` on a scrollbar tells: for a host that
    /// handles presses itself and leaves the ones that went to a scrollbar alone.
    pub fn is_scrollbar_dragged(&self) -> bool {
        self.bar.grab.get().is_some()
    }

    /// Whether the pointer is over the scrollbar or dragging it.
    pub(crate) fn scrollbar_active(&self) -> bool {
        self.bar.hovering.get() || self.bar.grab.get().is_some()
    }

    /// Records that the pointer entered or left the scrollbar's strip.
    pub(crate) fn set_scrollbar_hovered(&self, hovering: bool) {
        if self.bar.hovering.replace(hovering) != hovering {
            self.bar.hover_changed.set(Some(Instant::now()));
            self.show_scrollbar();
        }
    }

    /// The color of the thumb now: its resting color, its hover color while the pointer
    /// is over it or it is dragged, and between the two while it changes.
    pub(crate) fn scrollbar_color(&self) -> Color {
        let style = self.bar.style.get();
        let active = self.scrollbar_active();
        let duration = style.hover_duration.as_secs_f32();
        let done = self.bar.hover_changed.get().map_or(1.0, |at| {
            if duration <= 0.0 {
                1.0
            } else {
                (at.elapsed().as_secs_f32() / duration).clamp(0.0, 1.0)
            }
        });
        let (from, to) = if active {
            (style.color, style.active_color)
        } else {
            (style.active_color, style.color)
        };
        let mix =
            |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * done).round() as u8;
        Color {
            r: mix(from.r, to.r),
            g: mix(from.g, to.g),
            b: mix(from.b, to.b),
            a: mix(from.a, to.a),
        }
    }

    /// Handles the pointer at `position` along a track `track` long; the first
    /// call of a drag is the press. A press on the thumb grabs it where it was touched; a
    /// press elsewhere on the track centers the thumb under the pointer.
    pub(crate) fn drag_thumb(&self, position: f32, track: f32) {
        let first = self.bar.grab.get().is_none();
        let (content, viewport) = self.extent.get();
        let min = self.bar.style.get().min_length;
        let (start, length) = thumb(self.drawn_offset(), content, viewport, track, min);
        if first {
            let grab = if position >= start && position <= start + length {
                position - start
            } else {
                length / 2.0
            };
            self.bar.grab.set(Some(grab));
        }
        let grab = self.bar.grab.get().unwrap_or(length / 2.0);
        let drawn = offset_for_thumb(position - grab, content, viewport, track, min);
        if self.reverse_scrolling() {
            self.scroll_to(self.max_offset() - drawn);
        } else {
            self.scroll_to(drawn);
        }
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
        let bar = remember(|| Rc::new(Bar::new(tick, Some(frame_clock()))));
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

    /// A state made outside composition keeps its offset, cannot scroll past what layout
    /// found, and is equal only to its own handles.
    #[test]
    fn a_state_made_outside_composition_keeps_its_offset() {
        let state = ScrollState::new(50.0);
        assert_eq!(state.offset(), 50.0);
        state.set_extent(300.0, 100.0);
        state.scroll_to(500.0);
        assert_eq!(state.offset(), 200.0);
        assert_eq!(state.clone(), state);
        assert_ne!(state, ScrollState::new(50.0));
    }

    /// Counted from the end, the drawn offset is what is left of the room, and the wheel
    /// towards the end makes the offset smaller.
    #[test]
    fn a_reversed_state_counts_from_the_end() {
        let state = ScrollState::new(30.0);
        state.set_reverse_scrolling(true);
        state.set_extent(300.0, 100.0);
        assert_eq!(state.drawn_offset(), 170.0);
        state.scroll_drawn_by(20.0);
        assert_eq!(state.offset(), 10.0);
    }

    /// The thumb's color moves from its resting color toward its hover color.
    #[test]
    fn the_thumb_takes_its_hover_color_over_the_hover_duration() {
        let state = ScrollState::new(0.0);
        let style = ScrollbarStyle {
            hover_duration: Duration::ZERO,
            ..ScrollbarStyle::default()
        };
        state.set_scrollbar(style);
        assert_eq!(state.scrollbar_color(), style.color);
        state.set_scrollbar_hovered(true);
        assert_eq!(state.scrollbar_color(), style.active_color);
    }

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
