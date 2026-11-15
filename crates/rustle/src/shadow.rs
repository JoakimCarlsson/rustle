//! The shadow a window without the system's decorations casts outside its frame, where
//! the desktop does not count it as part of the window: on Wayland strips of surface laid
//! under the window's edges, and on X11 a band inside the window that the application
//! leaves for it, whose extents are told to the window manager so it places, snaps and
//! tiles the frame alone.

#[cfg(target_os = "linux")]
mod extents;
#[cfg(target_os = "linux")]
mod strips;

use std::cell::RefCell;
use std::collections::HashMap;

use winit::window::{Window, WindowId};

/// One layer of a [`WindowShadow`]: the window's rounded frame moved, grown and blurred.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowLayer {
    /// How far right the layer is moved, in logical pixels.
    pub x: f32,
    /// How far down the layer is moved, in logical pixels.
    pub y: f32,
    /// How far its edge fades, in logical pixels.
    pub blur: f32,
    /// How far it is grown on every side, in logical pixels.
    pub spread: f32,
    /// How opaque it is where it is solid, from 0 to 1.
    pub alpha: f32,
}

impl ShadowLayer {
    /// A layer moved by (`x`, `y`), faded over `blur`, grown by `spread` and `alpha` opaque.
    pub const fn new(x: f32, y: f32, blur: f32, spread: f32, alpha: f32) -> Self {
        Self {
            x,
            y,
            blur,
            spread,
            alpha,
        }
    }
}

/// The shadow a window without decorations casts around its frame while it floats, neither
/// maximized nor full screen.
///
/// On Wayland it is drawn on surfaces beside the window and needs nothing of the
/// application. On X11 the application leaves a band of [`WindowHandle::shadow_band`]
/// around its content and draws the shadow there itself, in a transparent window; the
/// window manager is told the band is not part of the frame.
///
/// [`WindowHandle::shadow_band`]: crate::WindowHandle::shadow_band
#[derive(Clone, Debug, PartialEq)]
pub struct WindowShadow {
    /// How far the shadow reaches out past the frame, in logical pixels.
    pub margin: f32,
    /// The radius of the frame's corners, in logical pixels.
    pub radius: f32,
    /// The layers of the shadow, laid over one another.
    pub layers: Vec<ShadowLayer>,
}

/// What shows the shadow of one window.
enum Shadow {
    /// Nothing does: the desktop cannot be told of it.
    Absent,
    /// Strips of surface laid under the edges of a Wayland window.
    #[cfg(target_os = "linux")]
    Strips(Box<strips::Strips>),
    /// The extents of the band an X11 window draws its shadow in, told to the window manager.
    #[cfg(target_os = "linux")]
    Extents(Box<extents::Extents>),
}

thread_local! {
    /// What shows the shadow of each window that casts one.
    static SHADOWS: RefCell<HashMap<WindowId, Shadow>> = RefCell::new(HashMap::new());
}

impl Shadow {
    /// What shows the shadow of `window` on the desktop it opened on.
    fn open(window: &dyn Window) -> Self {
        #[cfg(target_os = "linux")]
        {
            use winit::raw_window_handle::{
                HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
            };

            let display = window.display_handle().map(|handle| handle.as_raw());
            let surface = window.window_handle().map(|handle| handle.as_raw());
            match (display, surface) {
                (Ok(RawDisplayHandle::Wayland(display)), Ok(RawWindowHandle::Wayland(surface))) => {
                    if let Some(strips) = strips::Strips::open(display.display, surface.surface) {
                        return Self::Strips(Box::new(strips));
                    }
                }
                (_, Ok(RawWindowHandle::Xlib(handle))) => {
                    return Self::Extents(Box::new(extents::Extents::new(handle.window as u32)));
                }
                (_, Ok(RawWindowHandle::Xcb(handle))) => {
                    return Self::Extents(Box::new(extents::Extents::new(handle.window.get())));
                }
                _ => {}
            }
        }
        let _ = window;
        Self::Absent
    }

    /// Whether the application leaves a band around its content for this shadow.
    fn inside(&self) -> bool {
        match self {
            Self::Absent => false,
            #[cfg(target_os = "linux")]
            Self::Strips(_) => false,
            #[cfg(target_os = "linux")]
            Self::Extents(_) => true,
        }
    }

    /// Fits the shadow to `window` as it is now: `shadow` cast around its frame while it is
    /// `floating`, and gone while it is not.
    fn fit(&mut self, window: &dyn Window, shadow: &WindowShadow, floating: bool) {
        match self {
            Self::Absent => {
                let _ = (window, shadow, floating);
            }
            #[cfg(target_os = "linux")]
            Self::Strips(strips) => {
                let scale = window.scale_factor();
                let size = window.surface_size().to_logical::<f64>(scale);
                strips.fit(
                    (size.width.round() as u32, size.height.round() as u32),
                    scale,
                    floating.then_some(shadow),
                );
            }
            #[cfg(target_os = "linux")]
            Self::Extents(extents) => {
                let band = if floating {
                    (f64::from(shadow.margin) * window.scale_factor()).round() as u32
                } else {
                    0
                };
                extents.fit(band);
            }
        }
    }
}

/// Whether `window` floats: neither maximized nor full screen.
fn floating(window: &dyn Window) -> bool {
    !window.is_maximized() && window.fullscreen().is_none()
}

/// Fits the shadow of `window` to how it is now, as `shadow` describes it, opening what
/// shows it the first time; takes it away when `shadow` is `None`. Call it before each frame
/// is presented, so that a change shows with the frame.
pub(crate) fn fit(window: &dyn Window, shadow: Option<&WindowShadow>) {
    SHADOWS.with(|shadows| {
        let mut shadows = shadows.borrow_mut();
        match shadow {
            Some(shadow) => shadows
                .entry(window.id())
                .or_insert_with(|| Shadow::open(window))
                .fit(window, shadow, floating(window)),
            None => {
                if let Some(open) = shadows.get_mut(&window.id()) {
                    let gone = WindowShadow {
                        margin: 0.0,
                        radius: 0.0,
                        layers: Vec::new(),
                    };
                    open.fit(window, &gone, false);
                }
            }
        }
    });
}

/// How wide the band is that `window` leaves around its content for its shadow, in logical
/// pixels: `margin` while the window floats and its shadow is drawn inside it, as on X11,
/// and none otherwise.
pub(crate) fn band(window: &dyn Window, margin: f32) -> f32 {
    let inside = SHADOWS.with(|shadows| {
        shadows
            .borrow_mut()
            .entry(window.id())
            .or_insert_with(|| Shadow::open(window))
            .inside()
    });
    if inside && floating(window) {
        margin
    } else {
        0.0
    }
}
