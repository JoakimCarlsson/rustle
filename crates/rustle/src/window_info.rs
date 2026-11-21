//! What content knows about the window it is shown in, as Compose's `LocalWindowInfo` and
//! `LocalDensity` give it.

use std::rc::Rc;

use rustle_geometry::Size;
use rustle_runtime::{CompositionLocal, State, mutable_state};
use rustle_ui::{Density, LOCAL_DENSITY, LOCAL_URI_HANDLER, Modifiers, UriHandler};

/// What the window content is shown in is like now: Compose's `WindowInfo`.
///
/// Read it with [`LOCAL_WINDOW_INFO`]. Its values are state: content that reads one composes
/// again when it changes, and a drawing closure that reads one draws again.
#[derive(Clone)]
pub struct WindowInfo {
    held: Rc<Held>,
}

/// What a [`WindowInfo`] holds.
struct Held {
    focused: State<bool>,
    modifiers: State<Modifiers>,
    size: State<Size>,
    density: State<f32>,
}

impl WindowInfo {
    /// Information about a window that has keyboard focus, holds no modifier key, has no
    /// size yet and one physical pixel per logical one.
    pub(crate) fn new() -> Self {
        Self {
            held: Rc::new(Held {
                focused: mutable_state(true),
                modifiers: mutable_state(Modifiers::default()),
                size: mutable_state(Size::new(0.0, 0.0)),
                density: mutable_state(1.0),
            }),
        }
    }

    /// Whether the window has keyboard focus: Compose's `isWindowFocused`.
    pub fn is_window_focused(&self) -> bool {
        self.held.focused.get()
    }

    /// The modifier keys held: Compose's `keyboardModifiers`.
    pub fn keyboard_modifiers(&self) -> Modifiers {
        self.held.modifiers.get()
    }

    /// The size of the area content is laid out in, in logical pixels: Compose's
    /// `containerSize` (which Compose gives in physical pixels; multiply by
    /// [`LOCAL_DENSITY`](rustle_ui::LOCAL_DENSITY) for those).
    pub fn container_size(&self) -> Size {
        self.held.size.get()
    }

    /// Physical pixels per logical one where the window is.
    pub(crate) fn density(&self) -> f32 {
        self.held.density.get()
    }

    /// Records whether the window has keyboard focus.
    pub(crate) fn set_focused(&self, focused: bool) {
        set_changed(&self.held.focused, focused);
    }

    /// Records the modifier keys held.
    pub(crate) fn set_modifiers(&self, modifiers: Modifiers) {
        set_changed(&self.held.modifiers, modifiers);
    }

    /// Records the size content is laid out in.
    pub(crate) fn set_size(&self, size: Size) {
        set_changed(&self.held.size, size);
    }

    /// Records the physical pixels per logical one.
    pub(crate) fn set_density(&self, density: f32) {
        set_changed(&self.held.density, density);
    }
}

impl PartialEq for WindowInfo {
    /// Whether both describe the same window.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.held, &other.held)
    }
}

impl std::fmt::Debug for WindowInfo {
    /// The values it holds now.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowInfo")
            .field("is_window_focused", &self.held.focused.with(|v| *v))
            .field("keyboard_modifiers", &self.held.modifiers.with(|v| *v))
            .field("container_size", &self.held.size.with(|v| *v))
            .finish()
    }
}

/// Sets `state` to `value` when it differs, so readers are not told of a write that changes
/// nothing.
fn set_changed<T: Clone + PartialEq + 'static>(state: &State<T>, value: T) {
    if state.with(|current| *current != value) {
        state.set(value);
    }
}

/// The window the content is shown in: Compose's `LocalWindowInfo`. Every window, and every
/// embedded interface, provides its own.
pub static LOCAL_WINDOW_INFO: CompositionLocal<WindowInfo> = CompositionLocal::new(WindowInfo::new);

/// Runs `content` with `info` as the window it is shown in, and its density as
/// [`LOCAL_DENSITY`], with a [`LOCAL_URI_HANDLER`] that hands addresses to the system through
/// [`open_url`](crate::open_url); reading the density makes the caller compose again when it
/// changes.
pub(crate) fn provide_window<R>(info: &WindowInfo, content: impl FnOnce() -> R) -> R {
    let density = Density::new(info.density());
    let handler = SYSTEM_URI_HANDLER.with(UriHandler::clone);
    LOCAL_WINDOW_INFO.provide(info.clone(), || {
        LOCAL_DENSITY.provide(density, || LOCAL_URI_HANDLER.provide(handler, content))
    })
}

thread_local! {
    /// The handler that hands addresses to the system, shared so every window provides the same.
    static SYSTEM_URI_HANDLER: UriHandler = UriHandler::new(|uri| {
        crate::open_url(uri);
    });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use rustle_ui::{LOCAL_DENSITY, Modifiers};

    use super::*;
    use crate::testing::embedded;

    /// An interface 300 by 200 whose root records what it reads of the window each pass.
    #[test]
    fn content_reads_the_window_and_follows_its_changes() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let record = Rc::clone(&seen);
        let mut ui = embedded(300, 200, move || {
            let info = LOCAL_WINDOW_INFO.current();
            record.borrow_mut().push((
                info.is_window_focused(),
                info.keyboard_modifiers().shift,
                info.container_size(),
                LOCAL_DENSITY.current().density,
            ));
        });
        assert_eq!(
            seen.borrow().last().copied(),
            Some((true, false, Size::new(300.0, 200.0), 1.0))
        );
        ui.set_window_focused(false);
        ui.set_modifiers(Modifiers {
            shift: true,
            ..Modifiers::default()
        });
        ui.resize((600, 400), 2.0);
        ui.update();
        assert_eq!(
            seen.borrow().last().copied(),
            Some((false, true, Size::new(300.0, 200.0), 2.0))
        );
    }
}
