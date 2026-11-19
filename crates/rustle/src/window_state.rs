//! The state of a window an application reads and changes, as Compose Desktop's
//! `WindowState` holds it: its placement, whether it is minimized, where it is and how big.

use std::cell::Cell;
use std::rc::Rc;

use rustle_geometry::Size;
use rustle_runtime::{State, mutable_state};
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::monitor::Fullscreen;
use winit::window::Window;

/// How a window sits on the screen: Compose Desktop's `WindowPlacement`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum WindowPlacement {
    /// At its own position and size.
    #[default]
    Floating,
    /// Filling the screen's work area, with its frame.
    Maximized,
    /// Covering the whole screen, without a frame.
    Fullscreen,
}

/// Where a window is on the desktop: Compose Desktop's `WindowPosition`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum WindowPosition {
    /// Where the system puts it.
    #[default]
    PlatformDefault,
    /// Its top-left corner at `x`, `y`, in logical pixels on the desktop.
    Absolute {
        /// Logical pixels from the left of the desktop.
        x: f32,
        /// Logical pixels from the top of the desktop.
        y: f32,
    },
}

impl WindowPosition {
    /// The top-left corner at `x`, `y`: Compose's `WindowPosition(x, y)`.
    pub const fn new(x: f32, y: f32) -> Self {
        Self::Absolute { x, y }
    }
}

/// What a window was last set to or found to be, compared against what the state wants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placed {
    placement: WindowPlacement,
    is_minimized: bool,
    position: WindowPosition,
    size: Size,
}

/// What a [`WindowState`] holds.
struct Held {
    placement: State<WindowPlacement>,
    is_minimized: State<bool>,
    position: State<WindowPosition>,
    size: State<Size>,
    /// What the window was last set to or found to be; `None` until it opened.
    applied: Cell<Option<Placed>>,
}

/// The state of a window, read and changed by the application and kept up to date with what
/// the user does to the window: Compose Desktop's `WindowState`.
///
/// Give it to a window with [`WindowOptions::with_state`](crate::WindowOptions::with_state).
/// Its values are state, so content that reads them composes again when the window is
/// moved, resized, maximized or minimized; setting one changes the window. Cheap to clone;
/// clones are the same state.
#[derive(Clone)]
pub struct WindowState {
    held: Rc<Held>,
}

impl WindowState {
    /// A window state with `placement`, minimized or not, at `position`, `size` logical pixels
    /// in its content: Compose's `WindowState(placement, isMinimized, position, size)`.
    pub fn new(
        placement: WindowPlacement,
        is_minimized: bool,
        position: WindowPosition,
        size: Size,
    ) -> Self {
        Self {
            held: Rc::new(Held {
                placement: mutable_state(placement),
                is_minimized: mutable_state(is_minimized),
                position: mutable_state(position),
                size: mutable_state(size),
                applied: Cell::new(None),
            }),
        }
    }

    /// How the window sits on the screen.
    pub fn placement(&self) -> WindowPlacement {
        self.held.placement.get()
    }

    /// Floats, maximizes or makes the window full screen.
    pub fn set_placement(&self, placement: WindowPlacement) {
        set_changed(&self.held.placement, placement);
    }

    /// Whether the window is minimized.
    pub fn is_minimized(&self) -> bool {
        self.held.is_minimized.get()
    }

    /// Minimizes the window or restores it.
    pub fn set_minimized(&self, minimized: bool) {
        set_changed(&self.held.is_minimized, minimized);
    }

    /// Where the window is.
    pub fn position(&self) -> WindowPosition {
        self.held.position.get()
    }

    /// Moves the window. Wayland does not let a window move itself.
    pub fn set_position(&self, position: WindowPosition) {
        set_changed(&self.held.position, position);
    }

    /// The size of the window's content, in logical pixels.
    pub fn size(&self) -> Size {
        self.held.size.get()
    }

    /// Resizes the window's content to `size` logical pixels.
    pub fn set_size(&self, size: Size) {
        set_changed(&self.held.size, size);
    }

    /// What the state wants the window to be, read without being counted as read by a pass.
    fn wanted(&self) -> Placed {
        Placed {
            placement: self.held.placement.with(|value| *value),
            is_minimized: self.held.is_minimized.with(|value| *value),
            position: self.held.position.with(|value| *value),
            size: self.held.size.with(|value| *value),
        }
    }

    /// Records that the window was made as the state wants it, without changing it.
    pub(crate) fn opened(&self, window: &dyn Window) {
        let wanted = self.wanted();
        if wanted.is_minimized {
            window.set_minimized(true);
        }
        self.held.applied.set(Some(wanted));
    }

    /// Changes `window` where what the state wants differs from what the window was last set
    /// to or found to be.
    pub(crate) fn apply_to(&self, window: &dyn Window) {
        let wanted = self.wanted();
        let Some(applied) = self.held.applied.get() else {
            return self.opened(window);
        };
        if applied == wanted {
            return;
        }
        if applied.placement != wanted.placement {
            match wanted.placement {
                WindowPlacement::Fullscreen => {
                    window.set_fullscreen(Some(Fullscreen::Borderless(None)));
                }
                WindowPlacement::Maximized => {
                    window.set_fullscreen(None);
                    window.set_maximized(true);
                }
                WindowPlacement::Floating => {
                    window.set_fullscreen(None);
                    window.set_maximized(false);
                }
            }
        }
        if applied.is_minimized != wanted.is_minimized {
            window.set_minimized(wanted.is_minimized);
        }
        if applied.position != wanted.position
            && let WindowPosition::Absolute { x, y } = wanted.position
        {
            window.set_outer_position(LogicalPosition::new(x, y).into());
        }
        if applied.size != wanted.size {
            let _ = window.request_surface_size(
                LogicalSize::new(wanted.size.width, wanted.size.height).into(),
            );
        }
        self.held.applied.set(Some(wanted));
    }

    /// Reads what `window` is now into the state, after the user or the system moved,
    /// resized, maximized or minimized it.
    pub(crate) fn observe(&self, window: &dyn Window) {
        let wanted = self.wanted();
        let scale = window.scale_factor();
        let placement = if window.fullscreen().is_some() {
            WindowPlacement::Fullscreen
        } else if window.is_maximized() {
            WindowPlacement::Maximized
        } else {
            WindowPlacement::Floating
        };
        let is_minimized = window.is_minimized().unwrap_or(wanted.is_minimized);
        let position = window
            .outer_position()
            .ok()
            .map(|position| {
                let logical = position.to_logical::<f32>(scale);
                WindowPosition::new(logical.x, logical.y)
            })
            .unwrap_or(wanted.position);
        let surface = window.surface_size().to_logical::<f32>(scale);
        let size = if is_minimized || surface.width <= 0.0 || surface.height <= 0.0 {
            wanted.size
        } else {
            Size::new(surface.width, surface.height)
        };
        let now = Placed {
            placement,
            is_minimized,
            position,
            size,
        };
        self.held.applied.set(Some(now));
        set_changed(&self.held.placement, placement);
        set_changed(&self.held.is_minimized, is_minimized);
        set_changed(&self.held.position, position);
        if placement == WindowPlacement::Floating {
            set_changed(&self.held.size, size);
        }
    }
}

impl Default for WindowState {
    /// A floating window of 800 by 600 where the system puts it, as Compose's defaults.
    fn default() -> Self {
        Self::new(
            WindowPlacement::Floating,
            false,
            WindowPosition::PlatformDefault,
            Size::new(800.0, 600.0),
        )
    }
}

impl PartialEq for WindowState {
    /// Whether both are the same state.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.held, &other.held)
    }
}

impl std::fmt::Debug for WindowState {
    /// The values it holds now.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let wanted = self.wanted();
        f.debug_struct("WindowState")
            .field("placement", &wanted.placement)
            .field("is_minimized", &wanted.is_minimized)
            .field("position", &wanted.position)
            .field("size", &wanted.size)
            .finish()
    }
}

/// A window state remembered across recompositions: Compose Desktop's `rememberWindowState`.
#[track_caller]
pub fn remember_window_state(
    placement: WindowPlacement,
    is_minimized: bool,
    position: WindowPosition,
    size: Size,
) -> WindowState {
    rustle_runtime::remember(|| WindowState::new(placement, is_minimized, position, size)).get()
}

/// Sets `state` to `value` when it differs.
fn set_changed<T: Clone + PartialEq + 'static>(state: &State<T>, value: T) {
    if state.with(|current| *current != value) {
        state.set(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setters_change_the_state_and_clones_share_it() {
        let state = WindowState::default();
        let other = state.clone();
        assert_eq!(state, other);
        assert_ne!(state, WindowState::default());
        other.set_placement(WindowPlacement::Maximized);
        other.set_minimized(true);
        other.set_position(WindowPosition::new(10.0, 20.0));
        other.set_size(Size::new(640.0, 480.0));
        assert_eq!(state.placement(), WindowPlacement::Maximized);
        assert!(state.is_minimized());
        assert_eq!(
            state.position(),
            WindowPosition::Absolute { x: 10.0, y: 20.0 }
        );
        assert_eq!(state.size(), Size::new(640.0, 480.0));
    }
}
