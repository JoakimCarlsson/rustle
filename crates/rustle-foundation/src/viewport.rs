use std::cell::Cell;
use std::rc::Rc;

use rustle_geometry::Rect;
use rustle_runtime::remember;
use rustle_ui::{Modifier, canvas};
use rustle_unit::Dp;

/// Reserves an area of `width` by `height` that draws nothing, and tells `on_rect` where it
/// is, so the host of the interface can draw its own content there.
///
/// The rectangle is in logical pixels from the top-left corner of the area the interface
/// is drawn in. `on_rect` runs after layout whenever the rectangle differs from the one it was
/// last given, and once for the first layout, so it may set a `State` without looping.
/// The area passes the pointer to whatever is beneath it unless the modifier says otherwise.
///
/// ```ignore
/// let view = remember(|| Rect::default());
/// viewport(320.0, 180.0, { let view = view.clone(); move |rect| view.set(rect) });
/// ```
#[track_caller]
pub fn viewport(width: impl Into<Dp>, height: impl Into<Dp>, on_rect: impl Fn(Rect) + 'static) {
    viewport_with(Modifier::new().size(width, height), on_rect);
}

/// Like [`viewport`], sized and placed by `modifier`, such as `Modifier::new().weight(1.0)`
/// or `fill_max_size()`.
#[track_caller]
pub fn viewport_with(modifier: Modifier, on_rect: impl Fn(Rect) + 'static) {
    let reported = remember(|| Rc::new(Cell::new(None::<Rect>))).get();
    canvas(
        modifier.on_placed(move |rect, _| {
            if reported.get() != Some(rect) {
                reported.set(Some(rect));
                on_rect(rect);
            }
        }),
        |_| {},
    );
}
