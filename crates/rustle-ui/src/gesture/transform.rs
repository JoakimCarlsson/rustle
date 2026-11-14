//! Pan, zoom and rotation from several pointers at once.

use std::cell::RefCell;
use std::rc::Rc;

use rustle_geometry::Offset;
use rustle_runtime::{State, group, remember};

use super::{PointerId, PointerInput, PointerInputPhase};

/// How far the content is moved, scaled and rotated by a
/// [`Modifier::transformable`](crate::Modifier::transformable) area. Observable, so what
/// shows the content recomposes as it changes.
#[derive(Clone)]
pub struct TransformableState {
    offset: State<Offset>,
    zoom: State<f32>,
    rotation: State<f32>,
    previous: Rc<RefCell<Vec<(PointerId, Offset)>>>,
}

impl TransformableState {
    /// How far the content was panned, in logical pixels.
    pub fn offset(&self) -> Offset {
        self.offset.get()
    }

    /// The scale, 1 for the size it started at.
    pub fn zoom(&self) -> f32 {
        self.zoom.get()
    }

    /// The rotation in radians, positive clockwise.
    pub fn rotation(&self) -> f32 {
        self.rotation.get()
    }

    /// Applies a change: `zoom_change` multiplies the scale, `pan_change` moves the offset and
    /// `rotation_change` (radians) turns the content.
    pub fn transform_by(&self, zoom_change: f32, pan_change: Offset, rotation_change: f32) {
        self.zoom.update(|zoom| *zoom *= zoom_change);
        self.offset.update(|offset| {
            offset.x += pan_change.x;
            offset.y += pan_change.y;
        });
        self.rotation
            .update(|rotation| *rotation += rotation_change);
    }

    /// Back to no pan, a scale of 1 and no rotation.
    pub fn reset(&self) {
        self.zoom.set(1.0);
        self.offset.set(Offset::default());
        self.rotation.set(0.0);
    }

    /// Reads one event of the pointer stream and calls `on_transform` with the zoom
    /// factor, pan and rotation it caused. Returns whether it was used. A Ctrl+wheel
    /// zooms too.
    pub(crate) fn handle(
        &self,
        event: &PointerInput,
        on_transform: &dyn Fn(f32, Offset, f32),
    ) -> bool {
        if event.phase == PointerInputPhase::Scroll {
            if event.modifiers.ctrl && event.scroll_delta.y != 0.0 {
                on_transform((event.scroll_delta.y * 0.002).exp(), Offset::default(), 0.0);
                return true;
            }
            return false;
        }
        let now: Vec<(PointerId, Offset)> = event
            .pointers
            .iter()
            .map(|sample| (sample.id, sample.window_position))
            .collect();
        let before = self.previous.replace(now.clone());
        if event.phase != PointerInputPhase::Move {
            return false;
        }
        let common: Vec<(Offset, Offset)> = now
            .iter()
            .filter_map(|(id, at)| {
                before
                    .iter()
                    .find(|(old, _)| old == id)
                    .map(|(_, was)| (*was, *at))
            })
            .collect();
        if common.is_empty() {
            return false;
        }
        let count = common.len() as f32;
        let centroid = |points: &mut dyn Iterator<Item = Offset>| {
            let (x, y) = points.fold((0.0, 0.0), |(x, y), point| (x + point.x, y + point.y));
            Offset::new(x / count, y / count)
        };
        let was = centroid(&mut common.iter().map(|(was, _)| *was));
        let at = centroid(&mut common.iter().map(|(_, at)| *at));
        let pan = Offset::new(at.x - was.x, at.y - was.y);
        let (mut zoom, mut rotation) = (1.0, 0.0);
        if let [(a_was, a_at), (b_was, b_at), ..] = common[..] {
            let (was_x, was_y) = (b_was.x - a_was.x, b_was.y - a_was.y);
            let (at_x, at_y) = (b_at.x - a_at.x, b_at.y - a_at.y);
            let was_length = was_x.hypot(was_y);
            let at_length = at_x.hypot(at_y);
            if was_length > 0.0 && at_length > 0.0 {
                zoom = at_length / was_length;
                rotation = (at_y.atan2(at_x) - was_y.atan2(was_x) + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
            }
        }
        if zoom == 1.0 && rotation == 0.0 && pan == Offset::default() {
            return false;
        }
        on_transform(zoom, pan, rotation);
        true
    }
}

/// Remembers a [`TransformableState`] at this call site, with no pan, a scale of 1 and no rotation.
#[track_caller]
pub fn remember_transformable_state() -> TransformableState {
    group(|| {
        let offset = remember(Offset::default);
        let zoom = remember(|| 1.0);
        let rotation = remember(|| 0.0);
        let previous = remember(|| Rc::new(RefCell::new(Vec::new())));
        TransformableState {
            offset,
            zoom,
            rotation,
            previous: previous.get(),
        }
    })
}
