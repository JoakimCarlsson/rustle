//! Tests of window options, the registry composition fills, modal blocking and closing.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustle_runtime::{State, remember};
use winit::dpi::{LogicalPosition, LogicalSize, Size};
use winit::monitor::Fullscreen;
use winit::window::Theme;

use crate::window;
use crate::window::{MAIN, REGISTRY, WindowHandle, WindowOptions, WindowTheme, current_window};
use crate::windows::{close_requested, modal_over, wanted_windows};

/// Forgets every window the thread registered, for a test that starts clean.
fn clear_registry() {
    REGISTRY.with(|r| {
        let mut registry = r.borrow_mut();
        registry.entries.clear();
        registry.dirty = false;
        registry.close_requests.clear();
    });
}

/// The keys of the registered windows, in order.
fn keys() -> Vec<u64> {
    REGISTRY.with(|r| r.borrow().entries.keys().copied().collect())
}

/// The handle with `key`.
fn handle(key: u64) -> WindowHandle {
    WindowHandle { key }
}

#[test]
fn defaults_are_a_plain_visible_decorated_window() {
    let options = WindowOptions::default();
    assert_eq!(options.title, "Rustle");
    assert!(options.decorations && options.resizable && options.visible && options.ime_allowed);
    assert!(!options.modal && !options.closes_itself && !options.fullscreen);
    assert!(!options.transparent);
    assert_eq!(options.parent, None);
    assert_eq!(options.theme, None);
    assert_eq!(options.position, None);
    assert_eq!(options.app_id, None);
}

#[test]
fn the_with_methods_set_their_fields() {
    let parent = handle(4);
    let options = WindowOptions::new("Tool")
        .with_size(300.0, 200.0)
        .with_min_size(100.0, 50.0)
        .with_decorations(false)
        .with_resizable(false)
        .with_visible(false)
        .with_ime_allowed(false)
        .with_app_id("com.example.tool")
        .with_fullscreen(true)
        .with_position(10.0, 20.0)
        .with_transparent(true)
        .with_theme(WindowTheme::Dark)
        .with_modal_for(parent)
        .with_closes_itself(true);
    assert_eq!(options.title, "Tool");
    assert_eq!(options.size, (300.0, 200.0));
    assert_eq!(options.min_size, (100.0, 50.0));
    assert!(!options.decorations && !options.resizable && !options.visible);
    assert!(!options.ime_allowed);
    assert_eq!(options.app_id.as_deref(), Some("com.example.tool"));
    assert!(options.fullscreen && options.transparent && options.closes_itself);
    assert_eq!(options.position, Some((10.0, 20.0)));
    assert_eq!(options.theme, Some(WindowTheme::Dark));
    assert!(options.modal);
    assert_eq!(options.parent, Some(parent));
}

#[test]
fn a_modal_without_a_parent_has_none() {
    let options = WindowOptions::new("Alert").with_modal();
    assert!(options.modal);
    assert_eq!(options.parent, None);
}

#[test]
fn the_macos_title_bar_options_go_together() {
    let options = WindowOptions::new("Mac").with_macos_transparent_titlebar(true);
    assert!(options.macos_titlebar_transparent);
    assert!(options.macos_fullsize_content);
    assert!(options.macos_title_hidden);
    let shown = WindowOptions::new("Mac").with_macos_transparent_titlebar(false);
    assert!(shown.macos_titlebar_transparent && !shown.macos_title_hidden);
}

#[test]
fn themes_say_whether_they_are_dark() {
    assert!(WindowTheme::Dark.is_dark());
    assert!(!WindowTheme::Light.is_dark());
    assert_eq!(WindowTheme::Dark.winit(), Theme::Dark);
    assert_eq!(WindowTheme::Light.winit(), Theme::Light);
}

#[test]
fn options_become_window_attributes() {
    let options = WindowOptions::new("Inspector")
        .with_size(640.0, 480.0)
        .with_min_size(200.0, 100.0)
        .with_decorations(false)
        .with_resizable(false)
        .with_transparent(true)
        .with_theme(WindowTheme::Light)
        .with_position(30.0, 40.0)
        .with_fullscreen(true);
    let attributes = options.attributes(None);
    assert_eq!(attributes.title, "Inspector");
    assert!(!attributes.decorations);
    assert!(!attributes.resizable);
    assert!(attributes.transparent);
    assert_eq!(attributes.preferred_theme, Some(Theme::Light));
    assert_eq!(
        attributes.surface_size,
        Some(Size::Logical(LogicalSize::new(640.0, 480.0)))
    );
    assert_eq!(
        attributes.min_surface_size,
        Some(Size::Logical(LogicalSize::new(200.0, 100.0)))
    );
    assert_eq!(
        attributes.position,
        Some(LogicalPosition::new(30.0, 40.0).into())
    );
    assert!(matches!(
        attributes.fullscreen,
        Some(Fullscreen::Borderless(None))
    ));
}

/// A window kept in front opens at the level that keeps it there; others at the normal one.
#[test]
fn always_on_top_becomes_the_window_level() {
    let level = |options: WindowOptions| options.attributes(None).window_level;
    assert_eq!(
        level(WindowOptions::new("Picker").with_always_on_top(true)),
        winit::window::WindowLevel::AlwaysOnTop
    );
    assert_eq!(
        level(WindowOptions::new("Plain")),
        winit::window::WindowLevel::Normal
    );
}

#[test]
fn a_window_is_created_hidden_so_the_accessibility_adapter_comes_first() {
    for visible in [true, false] {
        let attributes = WindowOptions::new("W")
            .with_visible(visible)
            .attributes(None);
        assert!(!attributes.visible);
    }
}

#[test]
fn default_attributes_follow_the_system() {
    let attributes = WindowOptions::default().attributes(None);
    assert_eq!(attributes.preferred_theme, None);
    assert_eq!(attributes.position, None);
    assert!(attributes.fullscreen.is_none());
    assert!(attributes.decorations && attributes.resizable && !attributes.transparent);
}

#[test]
fn a_window_is_registered_with_its_options_and_content() {
    clear_registry();
    let mut composition = rustle_ui::composition(|| {
        window(
            WindowOptions::new("First").with_size(200.0, 100.0),
            || {},
            || crate::text("hello"),
        );
    });
    composition.compose();
    let entries = keys();
    assert_eq!(entries.len(), 1);
    REGISTRY.with(|r| {
        let registry = r.borrow();
        let entry = &registry.entries[&entries[0]];
        assert_eq!(entry.options.title, "First");
        assert_eq!(entry.options.size, (200.0, 100.0));
        assert!(entry.tree.is_some());
        assert!(entry.window.is_none());
        assert!(!entry.closed);
        assert!(registry.dirty);
    });
}

#[test]
fn changing_the_options_marks_the_registry_changed_and_keeps_the_window() {
    clear_registry();
    let title = Rc::new(RefCell::new("One".to_string()));
    let shown = Rc::clone(&title);
    let mut composition = rustle_ui::composition(move || {
        window(WindowOptions::new(shown.borrow().clone()), || {}, || {});
    });
    composition.compose();
    let key = keys()[0];
    REGISTRY.with(|r| r.borrow_mut().dirty = false);
    composition.invalidate();
    composition.recompose();
    assert!(!REGISTRY.with(|r| r.borrow().dirty), "nothing changed");
    *title.borrow_mut() = "Two".to_string();
    composition.invalidate();
    composition.recompose();
    assert_eq!(keys(), [key]);
    assert!(REGISTRY.with(|r| r.borrow().dirty));
    assert_eq!(
        REGISTRY.with(|r| r.borrow().entries[&key].options.title.clone()),
        "Two"
    );
}

#[test]
fn windows_get_distinct_keys_and_content_sees_its_own_handle() {
    clear_registry();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let kept = Rc::clone(&seen);
    let mut composition = rustle_ui::composition(move || {
        let kept = Rc::clone(&kept);
        window(
            WindowOptions::new("Outer"),
            || {},
            move || {
                let inner = Rc::clone(&kept);
                kept.borrow_mut().push(current_window());
                window(
                    WindowOptions::new("Inner"),
                    || {},
                    move || inner.borrow_mut().push(current_window()),
                );
                kept.borrow_mut().push(current_window());
            },
        );
    });
    composition.compose();
    let seen = seen.borrow();
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[0], seen[2], "the outer content is in the outer window");
    assert_ne!(seen[0], seen[1]);
    assert_ne!(seen[0], WindowHandle::MAIN);
    assert_eq!(keys().len(), 2);
}

#[test]
fn outside_every_window_the_current_window_is_the_main_one() {
    rustle_ui::composition(|| assert_eq!(current_window(), WindowHandle::MAIN)).compose();
    assert_eq!(WindowHandle::MAIN.key, MAIN);
}

#[test]
fn windows_from_one_call_site_are_told_apart_by_key() {
    clear_registry();
    let count: Rc<Cell<usize>> = Rc::new(Cell::new(2));
    let shown = Rc::clone(&count);
    let mut composition = rustle_ui::composition(move || {
        for id in 0..shown.get() {
            rustle_runtime::key(id, || {
                window(WindowOptions::new(format!("W{id}")), || {}, || {});
            });
        }
    });
    composition.compose();
    let before = keys();
    assert_eq!(before.len(), 2);
    count.set(3);
    composition.invalidate();
    composition.recompose();
    let after = keys();
    assert_eq!(after.len(), 3);
    assert_eq!(&after[..2], before.as_slice());
}

#[test]
fn a_window_stays_until_the_composition_stops_calling_it() {
    clear_registry();
    let closed = Rc::new(Cell::new(0));
    let open: Rc<Cell<bool>> = Rc::new(Cell::new(true));
    let (on_close, shown) = (Rc::clone(&closed), Rc::clone(&open));
    let mut composition = rustle_ui::composition(move || {
        if shown.get() {
            let on_close = Rc::clone(&on_close);
            window(
                WindowOptions::new("Stays"),
                move || on_close.set(on_close.get() + 1),
                || {},
            );
        }
    });
    composition.compose();
    let key = keys()[0];
    close_requested(key);
    assert_eq!(closed.get(), 1);
    assert_eq!(wanted_windows().len(), 1, "it is still wanted");
    open.set(false);
    composition.invalidate();
    composition.recompose();
    assert!(wanted_windows().is_empty());
}

#[test]
fn a_window_that_closes_itself_goes_at_once_and_stays_closed() {
    clear_registry();
    let closed = Rc::new(Cell::new(0));
    let on_close = Rc::clone(&closed);
    let mut composition = rustle_ui::composition(move || {
        let on_close = Rc::clone(&on_close);
        window(
            WindowOptions::new("Goes").with_closes_itself(true),
            move || on_close.set(on_close.get() + 1),
            || {},
        );
    });
    composition.compose();
    let key = keys()[0];
    REGISTRY.with(|r| r.borrow_mut().dirty = false);
    close_requested(key);
    assert_eq!(closed.get(), 1, "on_close_request still runs");
    assert!(wanted_windows().is_empty());
    assert!(
        REGISTRY.with(|r| r.borrow().dirty),
        "the host must look again"
    );
    composition.invalidate();
    composition.recompose();
    assert!(
        wanted_windows().is_empty(),
        "it does not reopen on the next pass"
    );
    assert_eq!(keys(), [key]);
}

#[test]
fn a_closed_window_reopens_when_its_call_leaves_and_comes_back() {
    clear_registry();
    let open: Rc<Cell<bool>> = Rc::new(Cell::new(true));
    let shown = Rc::clone(&open);
    let mut composition = rustle_ui::composition(move || {
        if shown.get() {
            window(
                WindowOptions::new("Again").with_closes_itself(true),
                || {},
                || {},
            );
        }
    });
    composition.compose();
    let first = keys()[0];
    close_requested(first);
    open.set(false);
    composition.invalidate();
    composition.recompose();
    assert!(keys().is_empty());
    open.set(true);
    composition.invalidate();
    composition.recompose();
    assert_eq!(wanted_windows().len(), 1);
    assert_ne!(wanted_windows()[0].0, first);
}

#[test]
fn closing_an_unknown_window_does_nothing() {
    clear_registry();
    close_requested(999);
    assert!(wanted_windows().is_empty());
}

#[test]
fn a_close_request_through_a_handle_is_queued() {
    clear_registry();
    handle(7).close();
    handle(8).close();
    let queued = REGISTRY.with(|r| std::mem::take(&mut r.borrow_mut().close_requests));
    assert_eq!(queued, [7, 8]);
}

#[test]
fn handles_to_windows_that_are_not_open_do_nothing() {
    clear_registry();
    let missing = handle(42);
    missing.minimize();
    missing.toggle_maximize();
    missing.set_fullscreen(true);
    missing.set_position(1.0, 2.0);
    missing.focus();
    missing.set_visible(false);
    missing.set_title("x");
    missing.drag_window();
    assert!(!missing.is_maximized());
    assert!(!missing.is_fullscreen());
    assert_eq!(missing.position(), None);
    assert!(REGISTRY.with(|r| r.borrow().cancel_press.is_empty()));
}

#[test]
fn the_main_window_is_never_wanted() {
    clear_registry();
    REGISTRY.with(|r| {
        r.borrow_mut().entries.insert(
            MAIN,
            crate::window::Entry {
                options: WindowOptions::default(),
                tree: None,
                on_close: None,
                window: None,
                closed: false,
                info: crate::window_info::WindowInfo::new(),
            },
        );
    });
    assert!(wanted_windows().is_empty());
}

/// The windows of a test, as `modal_over` takes them: each given as its registry key and
/// options, all open.
macro_rules! open {
    ($($window:expr),* $(,)?) => {
        vec![$(Some($window)),*]
    };
}

#[test]
fn nothing_blocks_windows_when_no_modal_is_open() {
    let (main, tool) = (WindowOptions::default(), WindowOptions::new("Tool"));
    let windows = open![(0, &main), (1, &tool)];
    assert_eq!(modal_over(&windows, 0), None);
    assert_eq!(modal_over(&windows, 1), None);
}

#[test]
fn a_modal_with_a_parent_blocks_only_that_parent() {
    let main = WindowOptions::default();
    let other = WindowOptions::new("Other");
    let dialog = WindowOptions::new("Dialog").with_modal_for(handle(0));
    let windows = open![(0, &main), (1, &other), (2, &dialog)];
    assert_eq!(modal_over(&windows, 0), Some(2));
    assert_eq!(modal_over(&windows, 1), None);
    assert_eq!(modal_over(&windows, 2), None);
}

#[test]
fn a_modal_without_a_parent_blocks_every_other_window() {
    let main = WindowOptions::default();
    let other = WindowOptions::new("Other");
    let alert = WindowOptions::new("Alert").with_modal();
    let windows = open![(0, &main), (1, &other), (2, &alert)];
    assert_eq!(modal_over(&windows, 0), Some(2));
    assert_eq!(modal_over(&windows, 1), Some(2));
    assert_eq!(modal_over(&windows, 2), None);
}

#[test]
fn a_window_that_is_not_open_blocks_nothing() {
    let main = WindowOptions::default();
    let alert = WindowOptions::new("Alert").with_modal();
    let windows = vec![Some((0, &main)), None, Some((2, &alert))];
    assert_eq!(modal_over(&windows, 0), Some(2));
    assert_eq!(modal_over(&windows, 1), None);
    let without = vec![Some((0, &main)), None];
    assert_eq!(modal_over(&without, 0), None);
}

#[test]
fn the_dialogs_of_a_modal_window_are_not_blocked_by_it() {
    let main = WindowOptions::default();
    let alert = WindowOptions::new("Alert").with_modal();
    let confirm = WindowOptions::new("Confirm").with_modal_for(handle(1));
    let windows = open![(0, &main), (1, &alert), (2, &confirm)];
    assert_eq!(modal_over(&windows, 2), None, "the dialog takes input");
    assert_eq!(
        modal_over(&windows, 1),
        Some(2),
        "the dialog blocks its parent"
    );
    assert_eq!(modal_over(&windows, 0), Some(1));
}

#[test]
fn descendants_of_a_modal_are_not_blocked_however_deep() {
    let main = WindowOptions::default();
    let alert = WindowOptions::new("Alert").with_modal();
    let child = WindowOptions::new("Child").with_modal_for(handle(1));
    let grandchild = WindowOptions::new("Grandchild").with_modal_for(handle(2));
    let windows = open![(0, &main), (1, &alert), (2, &child), (3, &grandchild)];
    assert_eq!(modal_over(&windows, 3), None);
    assert_eq!(modal_over(&windows, 2), Some(3));
    assert_eq!(modal_over(&windows, 1), Some(2));
}

#[test]
fn a_plain_window_with_a_parent_is_not_modal() {
    let main = WindowOptions::default();
    let mut child = WindowOptions::new("Child");
    child.parent = Some(handle(0));
    let windows = open![(0, &main), (1, &child)];
    assert_eq!(modal_over(&windows, 0), None);
}

#[test]
fn a_dialog_for_another_window_does_not_block_this_one() {
    let main = WindowOptions::default();
    let first = WindowOptions::new("First");
    let dialog = WindowOptions::new("Dialog").with_modal_for(handle(1));
    let windows = open![(0, &main), (1, &first), (2, &dialog)];
    assert_eq!(modal_over(&windows, 0), None);
    assert_eq!(modal_over(&windows, 1), Some(2));
}

#[test]
fn state_read_in_window_content_recomposes_the_window() {
    clear_registry();
    let state = Rc::new(RefCell::new(None::<State<i32>>));
    let kept = Rc::clone(&state);
    let mut composition = rustle_ui::composition(move || {
        let count = remember(|| 0);
        *kept.borrow_mut() = Some(count.clone());
        window(
            WindowOptions::new("Counter"),
            || {},
            move || crate::text(format!("{}", count.get())),
        );
    });
    composition.compose();
    let before = REGISTRY.with(|r| {
        format!(
            "{:?}",
            r.borrow().entries.values().next().unwrap().tree.is_some()
        )
    });
    assert_eq!(before, "true");
    state.borrow().as_ref().unwrap().set(5);
    assert!(composition.recompose());
}

#[test]
fn a_window_state_decides_how_the_window_opens() {
    let state = crate::WindowState::new(
        crate::WindowPlacement::Maximized,
        false,
        crate::WindowPosition::new(30.0, 40.0),
        rustle_geometry::Size::new(640.0, 480.0),
    );
    let options = WindowOptions::new("Stateful")
        .with_size(100.0, 100.0)
        .with_state(state.clone());
    let attributes = options.attributes(None);
    assert!(attributes.maximized);
    assert_eq!(
        attributes.surface_size,
        Some(winit::dpi::LogicalSize::new(640.0f32, 480.0).into())
    );
    assert_eq!(
        attributes.position,
        Some(winit::dpi::LogicalPosition::new(30.0f32, 40.0).into())
    );
    assert_eq!(
        options,
        WindowOptions::new("Stateful")
            .with_size(100.0, 100.0)
            .with_state(state)
    );
}

/// A window that is not open has no screen to tell of, and no size.
#[test]
fn a_window_that_is_not_open_tells_no_screen_size() {
    assert_eq!(handle(9_999).screen_size(), None);
    assert_eq!(handle(9_999).size(), None);
}
