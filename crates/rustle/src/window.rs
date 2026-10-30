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
use winit::window::{Window, WindowAttributes};

/// How a window looks and behaves when it opens.
///
/// Fields can be set with struct update syntax (`WindowOptions { decorations: false,
/// ..WindowOptions::default() }`) or with the `with_` methods. For a window opened with
/// [`window`], a changed title, decoration setting, resizability or minimum size is
/// applied to the open window; the other fields only matter when it opens.
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

    /// The winit attributes for a window with these options. It is made hidden, because the
    /// accessibility adapter has to exist before the window is shown.
    pub(crate) fn attributes(&self) -> WindowAttributes {
        let attributes = Window::default_attributes()
            .with_title(self.title.as_str())
            .with_visible(false)
            .with_decorations(self.decorations)
            .with_resizable(self.resizable)
            .with_inner_size(LogicalSize::new(self.size.0, self.size.1))
            .with_min_inner_size(LogicalSize::new(self.min_size.0, self.min_size.1));
        self.platform_attributes(attributes)
    }

    #[cfg(target_os = "macos")]
    fn platform_attributes(&self, attributes: WindowAttributes) -> WindowAttributes {
        use winit::platform::macos::WindowAttributesExtMacOS;
        attributes
            .with_titlebar_transparent(self.macos_titlebar_transparent)
            .with_fullsize_content_view(self.macos_fullsize_content)
            .with_title_hidden(self.macos_title_hidden)
    }

    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "android", target_os = "ios"))
    ))]
    fn platform_attributes(&self, attributes: WindowAttributes) -> WindowAttributes {
        use winit::platform::{wayland::WindowAttributesExtWayland, x11::WindowAttributesExtX11};
        match &self.app_id {
            Some(id) => {
                let attributes = WindowAttributesExtWayland::with_name(attributes, id, id);
                WindowAttributesExtX11::with_name(attributes, id, id)
            }
            None => attributes,
        }
    }

    #[cfg(not(any(
        target_os = "macos",
        all(
            unix,
            not(any(target_os = "macos", target_os = "android", target_os = "ios"))
        )
    )))]
    fn platform_attributes(&self, attributes: WindowAttributes) -> WindowAttributes {
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

    fn with_window<R>(self, f: impl FnOnce(&Window) -> R) -> Option<R> {
        let window = REGISTRY.with(|r| r.borrow().entries.get(&self.key)?.window.clone())?;
        Some(f(&window))
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
/// called. Windows do not close themselves: it should make the call stop happening,
/// typically by clearing a `State<bool>` that guards it. Changing `options` while the
/// window is open updates its title, decorations, resizability and minimum size.
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
        let tree = LOCAL_WINDOW.provide(handle, || rustle_ui::compose(content));
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
    pub(crate) window: Option<Arc<Window>>,
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
