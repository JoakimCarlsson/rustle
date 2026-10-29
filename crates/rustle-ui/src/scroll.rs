//! Scroll state shared between a scrolling modifier and the code driving it.

use rustle_runtime::{State, group, remember};

/// How far a scrolling area is scrolled, and how far it can go.
///
/// The offset is observable state, so changing it recomposes, which lets
/// content depend on the scroll position. The extent is filled in by layout.
#[derive(Clone)]
pub struct ScrollState {
    offset: State<f32>,
    /// Content length and viewport length along the scrolling axis.
    extent: State<(f32, f32)>,
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

    /// Scrolls by `delta`, stopping at either end.
    pub fn scroll_by(&self, delta: f32) {
        self.scroll_to(self.offset() + delta);
    }

    /// Scrolls to `offset`, stopping at either end.
    pub fn scroll_to(&self, offset: f32) {
        let offset = offset.clamp(0.0, self.max_offset());
        if offset != self.offset() {
            self.offset.set(offset);
        }
    }

    /// Records the lengths layout found. Changing them recomposes, so content
    /// that depends on the viewport, such as a lazy list, catches up.
    pub(crate) fn set_extent(&self, content: f32, viewport: f32) {
        if self.extent.get() != (content, viewport) {
            self.extent.set((content, viewport));
        }
    }
}

/// Remembers a [`ScrollState`] at this call site.
#[track_caller]
pub fn remember_scroll_state() -> ScrollState {
    group(|| ScrollState {
        offset: remember(|| 0.0),
        extent: remember(|| (0.0, 0.0)),
    })
}
