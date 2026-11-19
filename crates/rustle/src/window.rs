//! Windows: opening them from composition, describing them, and controlling them.
//!
//! The root of an application is shown in its first window. Any composable can open more
//! with [`window`]: the window is open for as long as the call is part of the composition.
//! Inside a window's content, [`current_window`] gives a [`WindowHandle`] for controlling
//! that window, for instance from a custom title bar.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use rustle_runtime::CompositionLocal;
use rustle_ui::Node;
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::monitor::Fullscreen;
use winit::window::{Window, WindowAttributes, WindowLevel};

/// The appearance a window is given, whatever the system prefers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowTheme {
    /// The light appearance.
    Light,
    /// The dark appearance.
    Dark,
}

impl WindowTheme {
    /// Whether this is the dark appearance.
    pub fn is_dark(self) -> bool {
        self == WindowTheme::Dark
    }

    /// The winit theme for this appearance.
    pub(crate) fn winit(self) -> winit::window::Theme {
        match self {
            WindowTheme::Light => winit::window::Theme::Light,
            WindowTheme::Dark => winit::window::Theme::Dark,
        }
    }
}

/// How a window looks and behaves when it opens.
///
/// Fields can be set with struct update syntax (`WindowOptions { decorations: false,
/// ..WindowOptions::default() }`) or with the `with_` methods. For a window opened with
/// [`window`], a changed title, decoration setting, resizability or minimum size is
/// applied to the open window, and so are `fullscreen`, `position`, `theme` and `modal`; the
/// other fields only matter when it opens.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowOptions {
    /// The window's title.
    pub title: String,
    /// The window's size when it opens, in logical pixels.
    pub size: (f32, f32),
    /// The least size the window can be given, in logical pixels.
    pub min_size: (f32, f32),
    /// Whether the system draws a title bar and border. Turn it off to draw your own and
    /// move the window with [`WindowHandle::drag_window`].
    pub decorations: bool,
    /// Whether the user can resize the window.
    pub resizable: bool,
    /// Whether the window is shown as soon as it is ready. A window that starts hidden is
    /// shown with [`WindowHandle::set_visible`].
    pub visible: bool,
    /// Whether the platform's input methods (composing text, emoji pickers) may be used
    /// while a text field has focus. When false they stay off.
    pub ime_allowed: bool,
    /// The application id on Wayland and the window class on X11, which desktops use to match
    /// the window to its `.desktop` file and icon. Ignored on other platforms.
    pub app_id: Option<String>,
    /// On macOS, draws the title bar without a background, so the content shows through it.
    /// Ignored on other platforms.
    pub macos_titlebar_transparent: bool,
    /// On macOS, lets the content extend under the title bar. Usually set together with
    /// [`macos_titlebar_transparent`](Self::macos_titlebar_transparent). Ignored on other
    /// platforms.
    pub macos_fullsize_content: bool,
    /// On macOS, hides the title text but keeps the title bar and its buttons. Ignored on
    /// other platforms.
    pub macos_title_hidden: bool,
    /// Whether the window covers the whole screen, without a title bar or border.
    pub fullscreen: bool,
    /// Where the window's top left corner opens, in logical pixels on the desktop. The
    /// system chooses when it is `None`. Wayland does not let a window place itself.
    pub position: Option<(f32, f32)>,
    /// Whether what is behind the window shows through where the interface is not opaque.
    /// The window needs a background that is not opaque to show anything.
    pub transparent: bool,
    /// The appearance of this window and its content, instead of the system's.
    pub theme: Option<WindowTheme>,
    /// Whether the window blocks input to its [`parent`](Self::parent), or to every other
    /// window when it has none, until it is closed. A modal window without a parent does not
    /// block the windows opened as modal dialogs of its own, directly or through other dialogs.
    pub modal: bool,
    /// The window this one belongs to, for [`modal`](Self::modal) windows and dialogs.
    ///
    /// Where the system has a parent that does not confine the child, the native window is
    /// made its child: an owned window above it on Windows, an attached window that moves
    /// with it on macOS. X11 only has parents that clip the child to the parent's area and
    /// Wayland has none through winit, so there it only decides which window the modal blocks.
    pub parent: Option<WindowHandle>,
    /// Whether the window goes away when the user closes it, instead of waiting for the
    /// composition to stop calling [`window`]. `on_close_request` still runs, so the state
    /// that guards the call can be cleared.
    pub closes_itself: bool,
    /// The shadow the window casts around its frame while it floats, when it draws its own
    /// frame without the system's decorations. See [`WindowShadow`](crate::WindowShadow).
    pub shadow: Option<crate::WindowShadow>,
    /// Whether the window stays in front of every window that is not also kept in front:
    /// Compose's `alwaysOnTop`. Where the desktop decides itself, as on Wayland, it may be
    /// ignored.
    pub always_on_top: bool,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "Rustle".to_string(),
            size: (1920.0, 1080.0),
            min_size: (360.0, 240.0),
            decorations: true,
            resizable: true,
            visible: true,
            ime_allowed: true,
            app_id: None,
            macos_titlebar_transparent: false,
            macos_fullsize_content: false,
            macos_title_hidden: false,
            fullscreen: false,
            position: None,
            transparent: false,
            theme: None,
            modal: false,
            parent: None,
            closes_itself: false,
            shadow: None,
            always_on_top: false,
        }
    }
}

impl WindowOptions {
    /// Default options with the given title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    /// Sets the size the window opens at, in logical pixels.
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.size = (width, height);
        self
    }

    /// Sets the least size the window can be given, in logical pixels.
    pub fn with_min_size(mut self, width: f32, height: f32) -> Self {
        self.min_size = (width, height);
        self
    }

    /// Sets whether the system draws the title bar and border.
    pub fn with_decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }

    /// Sets whether the user can resize the window.
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Sets whether the window is shown as soon as it is ready.
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Sets whether input methods may be used in the window.
    pub fn with_ime_allowed(mut self, ime_allowed: bool) -> Self {
        self.ime_allowed = ime_allowed;
        self
    }

    /// Sets the Wayland application id and X11 window class.
    pub fn with_app_id(mut self, app_id: impl Into<String>) -> Self {
        self.app_id = Some(app_id.into());
        self
    }

    /// Makes the macOS title bar transparent with content under it, and hides its title text
    /// if `hide_title`. Ignored on other platforms.
    pub fn with_macos_transparent_titlebar(mut self, hide_title: bool) -> Self {
        self.macos_titlebar_transparent = true;
        self.macos_fullsize_content = true;
        self.macos_title_hidden = hide_title;
        self
    }

    /// Sets whether the window covers the whole screen.
    pub fn with_fullscreen(mut self, fullscreen: bool) -> Self {
        self.fullscreen = fullscreen;
        self
    }

    /// Sets where the window's top left corner opens, in logical pixels.
    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.position = Some((x, y));
        self
    }

    /// Sets whether the window is see-through where the interface is not opaque.
    pub fn with_transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    /// Gives the window its own appearance instead of the system's.
    pub fn with_theme(mut self, theme: WindowTheme) -> Self {
        self.theme = Some(theme);
        self
    }

    /// Makes the window modal over `parent`.
    pub fn with_modal_for(mut self, parent: WindowHandle) -> Self {
        self.modal = true;
        self.parent = Some(parent);
        self
    }

    /// Makes the window modal over every other window.
    pub fn with_modal(mut self) -> Self {
        self.modal = true;
        self
    }

    /// Sets whether the window stays in front of the other windows.
    pub fn with_always_on_top(mut self, always_on_top: bool) -> Self {
        self.always_on_top = always_on_top;
        self
    }

    /// Makes the window close when the user closes it.
    pub fn with_closes_itself(mut self, closes_itself: bool) -> Self {
        self.closes_itself = closes_itself;
        self
    }

    /// The winit attributes for a window with these options, as a child of `parent` where
    /// the system has a notion of a parent that does not confine the window to it. It is
    /// made hidden, because the accessibility adapter has to exist before the window is shown.
    pub(crate) fn attributes(&self, parent: Option<&dyn Window>) -> WindowAttributes {
        let attributes = WindowAttributes::default()
            .with_title(self.title.as_str())
            .with_visible(false)
            .with_decorations(self.decorations)
            .with_resizable(self.resizable)
            .with_transparent(self.transparent)
            .with_theme(self.theme.map(WindowTheme::winit))
            .with_fullscreen(self.fullscreen.then_some(Fullscreen::Borderless(None)))
            .with_window_level(if self.always_on_top {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            })
            .with_surface_size(LogicalSize::new(self.size.0, self.size.1))
            .with_min_surface_size(LogicalSize::new(self.min_size.0, self.min_size.1));
        let attributes = match self.position {
            Some((x, y)) => attributes.with_position(winit::dpi::LogicalPosition::new(x, y)),
            None => attributes,
        };
        self.platform_attributes(attributes, parent)
    }

    #[cfg(target_os = "macos")]
    fn platform_attributes(
        &self,
        attributes: WindowAttributes,
        parent: Option<&dyn Window>,
    ) -> WindowAttributes {
        use winit::platform::macos::WindowAttributesMacOS;
        use winit::raw_window_handle::HasWindowHandle;
        let attributes = attributes.with_platform_attributes(Box::new(
            WindowAttributesMacOS::default()
                .with_titlebar_transparent(self.macos_titlebar_transparent)
                .with_fullsize_content_view(self.macos_fullsize_content)
                .with_title_hidden(self.macos_title_hidden),
        ));
        match parent.and_then(|parent| parent.window_handle().ok()) {
            // SAFETY: the handle comes from a live window of this application. On macOS the
            // child is a window attached to its parent, which moves with it and is not
            // confined to it.
            Some(handle) => unsafe { attributes.with_parent_window(Some(handle.as_raw())) },
            None => attributes,
        }
    }

    #[cfg(target_os = "windows")]
    fn platform_attributes(
        &self,
        attributes: WindowAttributes,
        parent: Option<&dyn Window>,
    ) -> WindowAttributes {
        use winit::platform::windows::WindowAttributesWindows;
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        match parent
            .and_then(|parent| parent.window_handle().ok())
            .map(|handle| handle.as_raw())
        {
            Some(RawWindowHandle::Win32(handle)) => attributes.with_platform_attributes(Box::new(
                WindowAttributesWindows::default().with_owner_window(handle.hwnd.get() as _),
            )),
            _ => attributes,
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn platform_attributes(
        &self,
        attributes: WindowAttributes,
        _parent: Option<&dyn Window>,
    ) -> WindowAttributes {
        attributes
    }
    /// `attributes` with the application id the desktop groups the window by, where the
    /// windowing system of `event_loop` has one: the Wayland app id or the X11 class.
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "android", target_os = "ios"))
    ))]
    pub(crate) fn named(
        &self,
        attributes: WindowAttributes,
        event_loop: &dyn ActiveEventLoop,
    ) -> WindowAttributes {
        use winit::platform::wayland::{ActiveEventLoopExtWayland, WindowAttributesWayland};
        use winit::platform::x11::WindowAttributesX11;
        let Some(id) = &self.app_id else {
            return attributes;
        };
        if event_loop.is_wayland() {
            attributes.with_platform_attributes(Box::new(
                WindowAttributesWayland::default().with_name(id, id),
            ))
        } else {
            attributes.with_platform_attributes(Box::new(
                WindowAttributesX11::default().with_name(id, id),
            ))
        }
    }

    /// `attributes` as they are: only Wayland and X11 name windows by an application id.
    #[cfg(not(all(
        unix,
        not(any(target_os = "macos", target_os = "android", target_os = "ios"))
    )))]
    pub(crate) fn named(
        &self,
        attributes: WindowAttributes,
        _event_loop: &dyn ActiveEventLoop,
    ) -> WindowAttributes {
        attributes
    }
}

/// An edge or corner a window can be resized from, for
/// [`WindowHandle::drag_resize_window`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizeEdge {
    /// The top edge.
    North,
    /// The bottom edge.
    South,
    /// The right edge.
    East,
    /// The left edge.
    West,
    /// The top right corner.
    NorthEast,
    /// The top left corner.
    NorthWest,
    /// The bottom right corner.
    SouthEast,
    /// The bottom left corner.
    SouthWest,
}

impl ResizeEdge {
    /// The cursor to show over this edge or corner.
    pub fn cursor(self) -> rustle_ui::CursorIcon {
        use rustle_ui::CursorIcon;
        match self {
            Self::North | Self::South => CursorIcon::NsResize,
            Self::East | Self::West => CursorIcon::EwResize,
            Self::NorthEast | Self::SouthWest => CursorIcon::NeswResize,
            Self::NorthWest | Self::SouthEast => CursorIcon::NwseResize,
        }
    }

    fn direction(self) -> winit::window::ResizeDirection {
        use winit::window::ResizeDirection as D;
        match self {
            Self::North => D::North,
            Self::South => D::South,
            Self::East => D::East,
            Self::West => D::West,
            Self::NorthEast => D::NorthEast,
            Self::NorthWest => D::NorthWest,
            Self::SouthEast => D::SouthEast,
            Self::SouthWest => D::SouthWest,
        }
    }
}

/// Controls one native window. Cheap to copy, so a composable takes it with
/// [`current_window`] and its event handlers keep it.
///
/// Every method does nothing if the window is not open (yet, or any more).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowHandle {
    pub(crate) key: u64,
}

/// The key of the window the root composable is shown in.
pub(crate) const MAIN: u64 = 0;

impl WindowHandle {
    /// The window the root composable is shown in.
    pub const MAIN: WindowHandle = WindowHandle { key: MAIN };

    fn with_window<R>(self, f: impl FnOnce(&dyn Window) -> R) -> Option<R> {
        let window = REGISTRY.with(|r| r.borrow().entries.get(&self.key)?.window.clone())?;
        Some(f(&*window))
    }

    /// Starts moving the window with the pointer, until the button is released. Call it from
    /// a press handler (`Modifier::on_press`) on a custom title bar.
    pub fn drag_window(self) {
        if self.with_window(|w| w.drag_window()).is_some() {
            self.cancel_press();
        }
    }

    /// Starts resizing the window from `edge`, until the button is released. Call it from a
    /// press handler on a border or corner area of an undecorated window.
    pub fn drag_resize_window(self, edge: ResizeEdge) {
        if self
            .with_window(|w| w.drag_resize_window(edge.direction()))
            .is_some()
        {
            self.cancel_press();
        }
    }

    /// The system takes over the pointer, so the release never arrives: forget the press.
    fn cancel_press(self) {
        REGISTRY.with(|r| r.borrow_mut().cancel_press.push(self.key));
    }

    /// The native window, while it is open, for what this handle does not offer, such as
    /// its level among other windows, asking for the user's attention, the input method's
    /// caret area or the window's id among the events of the event loop.
    pub fn native(self) -> Option<Arc<dyn Window>> {
        REGISTRY.with(|r| r.borrow().entries.get(&self.key)?.window.clone())
    }

    /// Minimizes the window.
    pub fn minimize(self) {
        self.with_window(|w| w.set_minimized(true));
    }

    /// Maximizes the window, or restores it if it is maximized.
    pub fn toggle_maximize(self) {
        self.with_window(|w| w.set_maximized(!w.is_maximized()));
    }

    /// Whether the window is maximized.
    pub fn is_maximized(self) -> bool {
        self.with_window(|w| w.is_maximized()).unwrap_or(false)
    }

    /// How wide the band is that the window leaves around its content for its shadow, in
    /// logical pixels: `margin` while it floats and the desktop wants the shadow drawn inside
    /// the window, as on X11, and none where the shadow is drawn beside the window, as on
    /// Wayland, or there is none. Draw the shadow in the band, in a transparent window.
    pub fn shadow_band(self, margin: f32) -> f32 {
        self.with_window(|w| crate::shadow::band(w, margin))
            .unwrap_or(0.0)
    }

    /// Makes the window cover the whole screen, or restores it.
    pub fn set_fullscreen(self, fullscreen: bool) {
        self.with_window(|w| w.set_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None))));
    }

    /// Whether the window covers the whole screen.
    pub fn is_fullscreen(self) -> bool {
        self.with_window(|w| w.fullscreen().is_some())
            .unwrap_or(false)
    }

    /// Moves the window's top left corner to `x`, `y`, in logical pixels on the desktop.
    /// Wayland does not let a window move itself.
    pub fn set_position(self, x: f32, y: f32) {
        self.with_window(|w| w.set_outer_position(winit::dpi::LogicalPosition::new(x, y).into()));
    }

    /// Where the window's top left corner is, in logical pixels on the desktop, if the
    /// window is open and the platform says.
    pub fn position(self) -> Option<(f32, f32)> {
        self.with_window(|w| {
            let position = w.outer_position().ok()?;
            let logical = position.to_logical::<f32>(w.scale_factor());
            Some((logical.x, logical.y))
        })
        .flatten()
    }

    /// How many physical pixels make one logical pixel on the window's screen; 1 when the
    /// window is not open.
    pub fn scale_factor(self) -> f32 {
        self.with_window(|w| w.scale_factor() as f32).unwrap_or(1.0)
    }

    /// The window's Wayland display and `wl_surface`, as raw pointers, when it is open on
    /// Wayland: for work the window does not do itself, such as surfaces laid beside it.
    /// The display stays open while the event loop runs and the surface while the window
    /// is open.
    pub fn wayland_surface(
        self,
    ) -> Option<(
        std::ptr::NonNull<std::ffi::c_void>,
        std::ptr::NonNull<std::ffi::c_void>,
    )> {
        use winit::raw_window_handle::{
            HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
        };
        self.with_window(|w| {
            let display = w.display_handle().ok()?.as_raw();
            let window = w.window_handle().ok()?.as_raw();
            match (display, window) {
                (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(window)) => {
                    Some((display.display, window.surface))
                }
                _ => None,
            }
        })
        .flatten()
    }

    /// The window's X11 window id, when it is open on X11. Xlib's window type is as wide
    /// as a C `unsigned long`, which is already `u32` on Windows.
    #[cfg_attr(windows, allow(clippy::useless_conversion))]
    pub fn x11_window(self) -> Option<u32> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        self.with_window(|w| match w.window_handle().ok()?.as_raw() {
            RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok(),
            RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
            _ => None,
        })
        .flatten()
    }

    /// Brings the window in front and gives it keyboard focus.
    pub fn focus(self) {
        self.with_window(|w| w.focus_window());
    }

    /// Shows or hides the window.
    pub fn set_visible(self, visible: bool) {
        self.with_window(|w| w.set_visible(visible));
    }

    /// Sets the window's title. For a window opened with [`window`] the title in its options
    /// is applied whenever it changes, and replaces this one.
    pub fn set_title(self, title: &str) {
        self.with_window(|w| w.set_title(title));
    }

    /// Asks to close the window, as if the user had used its close button. A secondary
    /// window calls the `on_close_request` it was opened with, which is expected to stop
    /// calling [`window`]; closing the main window ends the application.
    pub fn close(self) {
        REGISTRY.with(|r| r.borrow_mut().close_requests.push(self.key));
        wake();
    }
}

/// The window whose content is being composed, as a composition local.
static LOCAL_WINDOW: CompositionLocal<WindowHandle> = CompositionLocal::new(|| WindowHandle::MAIN);

/// The handle of the window the calling composable is shown in: the main window outside any
/// [`window`] content, otherwise the window of the innermost one.
///
/// Call it while composing and move the handle into event handlers.
pub fn current_window() -> WindowHandle {
    LOCAL_WINDOW.current()
}

/// Provides the main window's handle to `root`.
pub(crate) fn in_main_window<R>(root: impl FnOnce() -> R) -> R {
    LOCAL_WINDOW.provide(WindowHandle::MAIN, root)
}

/// Opens a native window showing `content` for as long as this call is part of the
/// composition, and closes it when the call leaves it (an untaken `if`, a removed list item).
///
/// The content composes with everything the surrounding composables provide, such as the
/// theme, and shares their state, but is laid out and drawn on its own, with its own size,
/// scale, pointer, hover and keyboard focus. Calls from one site, such as in a loop, are
/// told apart like any group: wrap each in `key(id, ..)`.
///
/// `on_close_request` runs when the user closes the window or [`WindowHandle::close`] is
/// called. Unless the options set `closes_itself`, the window stays until the call stops
/// happening, typically because `on_close_request` cleared a `State<bool>` that guards it.
/// A window that closes itself is gone as soon as the request is made and stays closed until
/// the call leaves the composition, so it does not reopen on the next pass. Changing
/// `options` while the window is open updates its title, decorations, resizability, minimum
/// size, fullscreen state, position, theme and modality.
///
/// A window with a `theme` composes its content with that appearance instead of the
/// system's.
///
/// ```ignore
/// let open = remember(|| true);
/// if open.get() {
///     window(WindowOptions::new("Inspector"), { let open = open.clone(); move || open.set(false) }, || {
///         text("Hello from a second window");
///     });
/// }
/// ```
///
/// Windows open when the event loop next idles after the composition pass, so a window
/// opened by a click appears right after it.
#[track_caller]
pub fn window(
    options: WindowOptions,
    on_close_request: impl Fn() + 'static,
    content: impl FnOnce(),
) {
    rustle_runtime::group(|| {
        let key = rustle_runtime::remember(next_key).get();
        let handle = WindowHandle { key };
        rustle_runtime::disposable_effect((), move || {
            move || REGISTRY.with(|r| r.borrow_mut().remove(key))
        });
        let theme = options.theme;
        let tree = LOCAL_WINDOW.provide(handle, || {
            rustle_ui::compose(|| match theme {
                Some(theme) => {
                    rustle_foundation::LOCAL_SYSTEM_DARK.provide(theme.is_dark(), content)
                }
                None => content(),
            })
        });
        REGISTRY.with(|r| {
            r.borrow_mut()
                .update(key, options, tree, Rc::new(on_close_request));
        });
    });
}

fn next_key() -> u64 {
    REGISTRY.with(|r| {
        let mut registry = r.borrow_mut();
        registry.next += 1;
        registry.next
    })
}

fn wake() {
    let wake = REGISTRY.with(|r| r.borrow().wake.clone());
    if let Some(wake) = wake {
        wake();
    }
}

/// What the composition says about one window.
pub(crate) struct Entry {
    pub(crate) options: WindowOptions,
    /// The latest composed content. Empty for the main window, whose tree the composition owns.
    pub(crate) tree: Option<Node>,
    pub(crate) on_close: Option<Rc<dyn Fn()>>,
    /// The native window, once the event loop has opened it.
    pub(crate) window: Option<Arc<dyn Window>>,
    /// Whether the user closed a window that closes itself, so it is not opened again.
    pub(crate) closed: bool,
}

/// The windows composition has asked for, and the requests waiting for the event loop.
///
/// Composition and the event loop share one thread, so this is how `window` talks to the
/// host without owning it.
#[derive(Default)]
pub(crate) struct Registry {
    pub(crate) entries: std::collections::BTreeMap<u64, Entry>,
    next: u64,
    /// Whether windows were added, removed or changed since the host last looked.
    pub(crate) dirty: bool,
    pub(crate) close_requests: Vec<u64>,
    pub(crate) cancel_press: Vec<u64>,
    wake: Option<Rc<dyn Fn()>>,
}

impl Registry {
    fn update(&mut self, key: u64, options: WindowOptions, tree: Node, on_close: Rc<dyn Fn()>) {
        match self.entries.get_mut(&key) {
            Some(entry) => {
                self.dirty |= entry.options != options;
                entry.options = options;
                entry.tree = Some(tree);
                entry.on_close = Some(on_close);
            }
            None => {
                self.dirty = true;
                self.entries.insert(
                    key,
                    Entry {
                        options,
                        tree: Some(tree),
                        on_close: Some(on_close),
                        window: None,
                        closed: false,
                    },
                );
            }
        }
    }

    fn remove(&mut self, key: u64) {
        self.dirty |= self.entries.remove(&key).is_some();
    }
}

thread_local! {
    pub(crate) static REGISTRY: RefCell<Registry> = RefCell::new(Registry {
        // Key 0 is the main window; others count up from 1.
        entries: std::collections::BTreeMap::new(),
        ..Registry::default()
    });
}

/// Lets `WindowHandle::close` wake the event loop.
pub(crate) fn set_wake(wake: Rc<dyn Fn()>) {
    REGISTRY.with(|r| r.borrow_mut().wake = Some(wake));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Vec<u64> {
        REGISTRY.with(|r| r.borrow().entries.keys().copied().collect())
    }

    #[test]
    fn window_is_registered_while_in_composition_and_removed_when_it_leaves() {
        let open = Rc::new(std::cell::Cell::new(true));
        let seen = Rc::new(std::cell::Cell::new(None));
        let (open_in, seen_in) = (Rc::clone(&open), Rc::clone(&seen));
        let mut composition = rustle_ui::composition(move || {
            if open_in.get() {
                let seen = Rc::clone(&seen_in);
                window(
                    WindowOptions::new("Second"),
                    || {},
                    move || seen.set(Some(current_window())),
                );
            }
        });
        composition.compose();
        let handle = seen.get().expect("content ran in the window");
        assert_ne!(handle, WindowHandle::MAIN);
        assert_eq!(keys(), vec![handle.key]);
        assert!(REGISTRY.with(|r| std::mem::take(&mut r.borrow_mut().dirty)));

        open.set(false);
        composition.invalidate();
        composition.recompose();
        assert!(keys().is_empty());
        assert!(REGISTRY.with(|r| r.borrow().dirty));
    }
}
