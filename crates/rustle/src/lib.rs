//! Rustle: a tiny declarative UI framework.

use std::borrow::Cow;
use std::cell::Cell;
use std::process;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Border, DrawCommand};
use rustle_ui::{Composition, DisplayList, PressEvent, take_focus_request};
use rustle_unit::Constraints;
use rustle_wgpu::Renderer;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{CursorIcon, Theme as WindowTheme, Window, WindowId};

mod accessibility;
mod input;
mod saved;

pub use rustle_animation::{
    Animatable, AnimationSpec, Easing, Lerp, animate_as_state, animate_color_as_state,
    animate_float_as_state, animate_value, infinite_float, infinite_float_repeating,
    remember_animatable, spring, tween,
};
pub use rustle_charts::{
    ChartStyle, Series, area_chart, bar_chart, bar_hover, chart_hover, donut_chart, line_chart,
    qr_code, sparkline,
};
pub use rustle_foundation::{
    Alignment, Arrangement, BoxAlignment, Canvas, ContentScale, LOCAL_TEXT_STYLE, LazyListState,
    Modifier, ScrollState, canvas, column, column_arranged, column_with, floating, flow_row, image,
    lazy_column, lazy_list, lazy_row, lazy_vertical_grid, remember_lazy_list_state, reveal,
    rich_text, rich_text_with, row, row_arranged, row_with, spacer, stack, stack_aligned,
    stack_with, text, text_styled, text_with,
};
pub use rustle_foundation::{
    Focus, Key, KeyEvent, LOCAL_LAYOUT_DIRECTION, LOCAL_SYSTEM_DARK, LayoutDirection, Modifiers,
    Role, Semantics, TextFieldOptions, TextFieldState, TextFieldValue, basic_text_area,
    basic_text_field, basic_text_field_in, is_rtl, is_system_in_dark_theme, remember_focus,
    remember_scroll_state, remember_text_field_state, selectable_text,
};
pub use rustle_graphics::{Color, ImageData, TextAlign};
pub use rustle_macros::composable;
pub use rustle_navigation::{NavController, nav_host, remember_nav_controller};
pub use rustle_runtime::{
    CompositionLocal, State, StateList, StateMap, TaskScope, derived_state_of, disposable_effect,
    group, key, launched_effect, memo, produce_state, remember, remember_saveable,
    remember_state_list, remember_state_map, remember_updated_state, side_effect,
};
pub use rustle_text::{FontFamily, FontWeight, TextSpan, TextStyle};
pub use rustle_ui::skippable;
pub use rustle_unit::{Dp, dp};
pub use rustle_widgets::{
    BadgeStyle, ButtonStyle, Icon, MenuItem, PagerState, Theme, alert_dialog, animate_scroll_to,
    animated_visibility, avatar, badge, bottom_sheet, button, button_with, calendar, card,
    card_header, checkbox, chip, circular_progress_indicator,
    circular_progress_indicator_indeterminate, crossfade, days_in_month, dialog, divider,
    dropdown_menu, dropdown_menu_item, first_weekday, floating_action_button, horizontal_pager,
    icon, icon_button, linear_progress_indicator, list_item, menu, muted, muted_sized,
    navigation_bar, outlined_button, page_indicator, pill, popup, provide_theme, radio_button,
    range_slider, remember_pager_state, scaffold, segmented_control, select, select_field,
    separator, slider, snackbar, styled_button, surface, surface_with, swipe_to_dismiss, switch,
    system_theme, tab_row, table_cell, table_row, tabs_list, text_area, text_button, text_field,
    text_field_focused, theme, title, tooltip, top_app_bar,
};

pub mod prelude {
    pub use crate::{
        Alignment, Animatable, AnimationSpec, Arrangement, BadgeStyle, BoxAlignment, ButtonStyle,
        Canvas, ChartStyle, Color, CompositionLocal, ContentScale, Dp, Easing, Focus, FontFamily,
        FontWeight, Icon, ImageData, LOCAL_LAYOUT_DIRECTION, LOCAL_TEXT_STYLE, LayoutDirection,
        LazyListState, MenuItem, Modifier, NavController, PagerState, Role, ScrollState, Semantics,
        Series, State, StateList, StateMap, TaskScope, TextAlign, TextFieldState, TextSpan,
        TextStyle, Theme, alert_dialog, animate_color_as_state, animate_float_as_state,
        animate_value, animated_visibility, area_chart, avatar, badge, bar_chart, bar_hover,
        bottom_sheet, button, button_with, calendar, canvas, card, card_header, chart_hover,
        checkbox, chip, circular_progress_indicator, circular_progress_indicator_indeterminate,
        column, column_arranged, column_with, composable, crossfade, days_in_month,
        derived_state_of, dialog, disposable_effect, divider, donut_chart, dp, dropdown_menu,
        dropdown_menu_item, first_weekday, floating, floating_action_button, flow_row,
        horizontal_pager, icon, icon_button, image, infinite_float, infinite_float_repeating,
        is_rtl, is_system_in_dark_theme, key, launched_effect, lazy_column, lazy_list, lazy_row,
        lazy_vertical_grid, line_chart, linear_progress_indicator, list_item, menu, muted,
        muted_sized, nav_host, navigation_bar, outlined_button, page_indicator, pill, popup,
        produce_state, provide_theme, qr_code, radio_button, range_slider, remember,
        remember_animatable, remember_focus, remember_lazy_list_state, remember_nav_controller,
        remember_pager_state, remember_saveable, remember_scroll_state, remember_state_list,
        remember_state_map, remember_text_field_state, remember_updated_state, reveal, rich_text,
        rich_text_with, row, row_arranged, row_with, scaffold, segmented_control, select,
        select_field, selectable_text, separator, side_effect, slider, snackbar, spacer, sparkline,
        spring, stack, stack_aligned, stack_with, styled_button, surface, surface_with,
        swipe_to_dismiss, switch, system_theme, tab_row, table_cell, table_row, tabs_list, text,
        text_area, text_button, text_field, text_field_focused, text_styled, text_with, theme,
        title, tooltip, top_app_bar, tween,
    };
}

/// What wakes the event loop.
enum UserEvent {
    /// State changed or a frame was requested.
    Wake,
    /// The platform's accessibility API asked for something.
    Accessibility(accesskit_winit::Event),
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Accessibility(event)
    }
}

/// Composes `root` and shows it in a native window until closed.
///
/// The whole root is composed again, on the event loop, whenever state it
/// remembers has changed. Animations and tasks that wait on frames keep the
/// loop drawing for as long as they run; otherwise it sleeps until something happens.
///
/// Exits the process with status 1 if the window or renderer cannot be set up.
pub fn run(mut root: impl FnMut() + 'static) {
    // Developer tool: draw one frame without a window and write it as a PNG.
    //   RUSTLE_SNAPSHOT=out.png [RUSTLE_SNAPSHOT_SIZE=1280x800] [RUSTLE_SNAPSHOT_LIGHT=1]
    //   [RUSTLE_SNAPSHOT_INPUT="click:x,y;type:text;key:Backspace"] replays input first
    if let Some(path) = std::env::var_os("RUSTLE_SNAPSHOT") {
        rustle_runtime::set_saver(Rc::new(saved::FileSaver::open()));
        let size = std::env::var("RUSTLE_SNAPSHOT_SIZE")
            .ok()
            .and_then(|size| {
                let (width, height) = size.split_once('x')?;
                Some((width.parse().ok()?, height.parse().ok()?))
            })
            .unwrap_or((1280, 800));
        let dark = std::env::var_os("RUSTLE_SNAPSHOT_LIGHT").is_none();
        if let Err(error) = snapshot(root, dark, size, path.as_ref()) {
            eprintln!("rustle: {error}");
            process::exit(1);
        }
        return;
    }
    rustle_runtime::set_saver(Rc::new(saved::FileSaver::open()));
    let dark = Rc::new(Cell::new(true));
    let provided = {
        let dark = Rc::clone(&dark);
        move || rustle_foundation::LOCAL_SYSTEM_DARK.provide(dark.get(), &mut root)
    };
    let mut app = App {
        dark,
        composition: rustle_ui::composition(provided),
        started: Instant::now(),
        display: DisplayList::default(),
        pointer: None,
        focused: None,
        typed: None,
        debug_input: std::env::var_os("RUSTLE_DEBUG_INPUT").is_some(),
        long_press: None,
        hovered: Vec::new(),
        following: Vec::new(),
        dragging: None,
        carrying: None,
        modifiers: Modifiers::default(),
        last_press: None,
        pressed: None,
        proxy: None,
        window: None,
        error: None,
    };

    let result = EventLoop::<UserEvent>::with_user_event()
        .build()
        .and_then(|event_loop| {
            // State changes wake the event loop instead of being polled for. A
            // new composition is dirty, so the first wake-up also composes it.
            let proxy = event_loop.create_proxy();
            let wake = proxy.clone();
            app.composition.on_invalidate(move || {
                let _ = wake.send_event(UserEvent::Wake);
            });
            // Animations ask for frames; only then does the loop keep redrawing.
            let wake = proxy.clone();
            app.composition.on_frame_request(move || {
                let _ = wake.send_event(UserEvent::Wake);
            });
            app.proxy = Some(proxy.clone());
            let _ = proxy.send_event(UserEvent::Wake);
            event_loop.set_control_flow(ControlFlow::Wait);
            event_loop.run_app(&mut app)
        });
    if let Err(error) = result {
        app.error = Some(error.to_string());
    }
    if let Some(error) = app.error {
        eprintln!("rustle: {error}");
        process::exit(1);
    }
}

/// An area following the pointer, and what to tell it when the pointer leaves.
type FollowEntry = (Rect, Rc<dyn Fn(Option<Offset>)>);

/// An area the pointer is over, and what to tell it when the pointer leaves.
type HoverEntry = (Rect, Rc<dyn Fn(bool)>);

/// A press that becomes a long press if it is held: when, what, and where it began.
type LongPress = (Instant, Rc<dyn Fn()>, Offset);

/// Something picked up by a drag source: what it carries, where the press began, and
/// whether the pointer has moved far enough for it to count as a drag.
struct Carrying {
    payload: Rc<dyn std::any::Any>,
    began: Offset,
    active: bool,
    /// Where the picked-up area is, and which of the display list's commands draw it.
    rect: Rect,
    commands: std::ops::Range<usize>,
}

/// A drag in progress: its handler, and where its area's corner is.
type Drag = (Rc<dyn Fn(Offset)>, Offset, Option<Rc<dyn Fn()>>);

struct App {
    /// Whether the system is set to a dark appearance, as the root sees it.
    dark: Rc<Cell<bool>>,
    composition: Composition,
    started: Instant,
    display: DisplayList,
    /// Where the pointer is, in logical pixels, while it is over the window.
    pointer: Option<Offset>,
    /// What has keyboard focus.
    focused: Option<Focus>,
    /// Text a key press just typed, so the same text arriving again from an input method is not typed twice.
    typed: Option<(Instant, String)>,
    /// Whether to print the input the window receives (`RUSTLE_DEBUG_INPUT`).
    debug_input: bool,
    /// A long press waiting for its time: when it is due, what to run, and where it began.
    long_press: Option<LongPress>,
    /// The hover handlers of the areas the pointer is over.
    hovered: Vec<HoverEntry>,
    /// The areas whose pointer-move handlers have been told where the pointer is.
    following: Vec<FollowEntry>,
    /// The handler following a drag, and where its area's corner is.
    dragging: Option<Drag>,
    /// What was picked up with `drag_source`, and where, until the button is released.
    carrying: Option<Carrying>,
    modifiers: Modifiers,
    /// When and where the primary button last went down, and how many in a row.
    last_press: Option<(Instant, Offset, u32)>,
    /// The handler of the region the primary button went down on.
    /// Where the primary button went down on something clickable. Areas are
    /// told apart by where they are, not by their handlers, because every
    /// recomposition makes new handlers, and an animation recomposes every frame.
    pressed: Option<Rect>,
    proxy: Option<EventLoopProxy<UserEvent>>,
    window: Option<WindowState>,
    error: Option<String>,
}

struct WindowState {
    adapter: accesskit_winit::Adapter,
    renderer: Renderer,
    window: Arc<Window>,
}

impl App {
    /// Lays the latest tree out for the current window and asks for a redraw.
    ///
    /// Text is measured by the renderer, so this waits until the window exists
    /// and the first pass has produced a tree.
    fn relayout(&mut self) {
        let (Some(state), Some(tree)) = (&mut self.window, self.composition.output()) else {
            return;
        };
        let scale_factor = state.window.scale_factor() as f32;
        let size = state.window.inner_size();
        let constraints = Constraints::loose(Size::new(
            size.width as f32 / scale_factor,
            size.height as f32 / scale_factor,
        ));
        self.display = rustle_ui::display_list(tree, constraints, &mut state.renderer);
        state.window.request_redraw();
        self.update_hover();
        self.update_accessibility();
    }

    /// Records the system appearance and recomposes if it changed.
    fn set_dark(&mut self, dark: bool) {
        if self.dark.replace(dark) != dark {
            self.composition.invalidate();
        }
    }

    /// Tells the platform what the latest layout means, if anything is listening.
    fn update_accessibility(&mut self) {
        let Some(state) = &mut self.window else {
            return;
        };
        let scale_factor = state.window.scale_factor();
        let (display, focused) = (&self.display, self.focused.as_ref());
        state
            .adapter
            .update_if_active(|| accessibility::tree(display, scale_factor, focused));
    }

    fn accessibility_event(&mut self, event: accesskit_winit::WindowEvent) {
        use accesskit::Action;
        match event {
            accesskit_winit::WindowEvent::InitialTreeRequested => self.update_accessibility(),
            accesskit_winit::WindowEvent::ActionRequested(request) => {
                let described = accessibility::index_of(request.target_node)
                    .and_then(|index| self.display.semantics.get(index));
                match request.action {
                    Action::Click => {
                        if let Some(on_click) = described.and_then(|node| node.on_click.clone()) {
                            on_click();
                        }
                    }
                    Action::Focus => {
                        let focus = described.and_then(|node| node.semantics.focus.clone());
                        self.set_focus(focus);
                    }
                    _ => {}
                }
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {}
        }
    }

    /// Moves focus to `focus`, telling the old and new owners.
    fn set_focus(&mut self, focus: Option<Focus>) {
        let same = match (&self.focused, &focus) {
            (Some(old), Some(new)) => old.id() == new.id(),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        if let Some(old) = &self.focused {
            old.set_focused(false);
        }
        if let Some(new) = &focus {
            new.set_focused(true);
        }
        self.focused = focus;
        // An input method only gets keys while something wants text, so it
        // cannot take keys from the rest of the application.
        if let Some(state) = &self.window {
            state.window.set_ime_allowed(self.focused.is_some());
        }
    }

    fn apply_focus_request(&mut self) {
        if let Some(focus) = take_focus_request() {
            self.set_focus(Some(focus));
        }
    }

    /// Gives a key event to the focused area, then does its default if unhandled:
    /// Tab moves focus along and Escape drops it.
    fn key_event(&mut self, event: KeyEvent) {
        let handler = self.focused.as_ref().and_then(|focus| {
            self.display
                .focusables
                .iter()
                .find(|region| region.focus.id() == focus.id())
                .map(|region| Rc::clone(&region.on_key))
        });
        if let Some(handler) = handler
            && handler(&event)
        {
            return;
        }
        if !event.pressed {
            return;
        }
        match event.key {
            Key::Tab => {
                let regions = &self.display.focusables;
                let current = self
                    .focused
                    .as_ref()
                    .and_then(|focus| regions.iter().position(|r| r.focus.id() == focus.id()));
                let count = regions.len();
                let next = match (current, event.modifiers.shift) {
                    _ if count == 0 => None,
                    (None, false) => Some(0),
                    (None, true) => Some(count - 1),
                    (Some(index), false) => Some((index + 1) % count),
                    (Some(index), true) => Some((index + count - 1) % count),
                };
                let next = next.map(|index| regions[index].focus.clone());
                self.set_focus(next);
            }
            Key::Escape => self.set_focus(None),
            _ => {}
        }
    }

    /// Runs what animations asked for at this frame and applies the state
    /// changes it caused, so the frame about to be drawn shows them.
    fn run_frame(&mut self) {
        if self.composition.has_pending_frame() {
            self.composition.run_frame(self.started.elapsed());
        }
        if self.composition.recompose() {
            self.relayout();
        }
    }

    /// Keeps frames coming for as long as something wants one.
    fn request_redraw_for_frames(&self) {
        if self.composition.has_pending_frame()
            && let Some(state) = &self.window
        {
            state.window.request_redraw();
        }
    }

    /// Tells the areas that follow the pointer where it is, or that it left.
    fn update_following(&mut self) {
        let mut now: Vec<(FollowEntry, Offset)> = Vec::new();
        if let Some(pointer) = self.pointer {
            for region in &self.display.pointer_moves {
                let inside = pointer.x >= region.rect.origin.x
                    && pointer.x < region.rect.origin.x + region.rect.size.width
                    && pointer.y >= region.rect.origin.y
                    && pointer.y < region.rect.origin.y + region.rect.size.height;
                if inside {
                    let local =
                        Offset::new(pointer.x - region.origin.x, pointer.y - region.origin.y);
                    now.push(((region.rect, Rc::clone(&region.on_move)), local));
                }
            }
        }
        for (rect, on_move) in &self.following {
            if !now
                .iter()
                .any(|((current, _), _)| same_area(*current, *rect))
            {
                on_move(None);
            }
        }
        for ((_, on_move), local) in &now {
            on_move(Some(*local));
        }
        self.following = now.into_iter().map(|(entry, _)| entry).collect();
    }

    /// Tells the areas the pointer entered or left.
    fn update_hover(&mut self) {
        let now: Vec<HoverEntry> = match self.pointer {
            Some(pointer) => self
                .display
                .hovers_at(pointer)
                .map(|region| (region.rect, Rc::clone(&region.on_hover)))
                .collect(),
            None => Vec::new(),
        };
        for (rect, on_hover) in &self.hovered {
            if !now.iter().any(|(current, _)| same_area(*current, *rect)) {
                on_hover(false);
            }
        }
        for (rect, on_hover) in &now {
            if !self.hovered.iter().any(|(old, _)| same_area(*old, *rect)) {
                on_hover(true);
            }
        }
        self.hovered = now;
    }

    /// The pointer moved to `pointer`, in logical pixels.
    fn pointer_moved(&mut self, pointer: Offset) {
        let Some(window) = self.window.as_ref().map(|state| Arc::clone(&state.window)) else {
            return;
        };
        let before = hover_target(&self.display, self.pointer);
        let clickable = self.display.hit_test(pointer).is_some();
        window.set_cursor(if clickable {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        });
        self.pointer = Some(pointer);
        self.update_following();
        if let Some(carrying) = &mut self.carrying
            && !carrying.active
            && ((carrying.began.x - pointer.x).abs() > 6.0
                || (carrying.began.y - pointer.y).abs() > 6.0)
        {
            carrying.active = true;
        }
        if self
            .carrying
            .as_ref()
            .is_some_and(|carrying| carrying.active)
        {
            window.set_cursor(CursorIcon::Grabbing);
            window.request_redraw();
        }
        // Moving away from where the press began is not a long press.
        if let Some((_, _, began)) = &self.long_press
            && ((began.x - pointer.x).abs() > 8.0 || (began.y - pointer.y).abs() > 8.0)
        {
            self.long_press = None;
        }
        self.update_hover();
        if let Some((on_drag, origin, _)) = self.dragging.clone() {
            on_drag(Offset::new(pointer.x - origin.x, pointer.y - origin.y));
        }
        if before != hover_target(&self.display, self.pointer) {
            window.request_redraw();
        }
    }

    /// The primary button, or a finger, went down or up where the pointer is.
    fn primary_button(&mut self, pressed: bool) {
        if self.debug_input {
            let here = self
                .pointer
                .and_then(|pointer| self.display.hit_test(pointer));
            eprintln!(
                "rustle input: button {} at {:?}; clickable here: {}; clickable areas: {}, focusable: {}",
                if pressed { "down" } else { "up" },
                self.pointer,
                here.is_some(),
                self.display.regions.len(),
                self.display.focusables.len(),
            );
        }
        let target = self
            .pointer
            .and_then(|pointer| self.display.hit_test(pointer))
            .map(|region| region.rect);
        if let Some(state) = &self.window {
            state.window.request_redraw();
        }
        if !pressed {
            // Something carried over a drop target is dropped on it.
            let carrying = self.carrying.take();
            if let Some(carrying) = carrying
                && carrying.active
                && let Some(pointer) = self.pointer
                && let Some(region) = self.display.drop_target_at(pointer)
            {
                let on_drop = Rc::clone(&region.on_drop);
                on_drop(carrying.payload.as_ref());
                self.pressed = None;
            }
            // A click is a press and a release on the same region.
            if let Some((_, _, Some(on_end))) = self.dragging.take() {
                on_end();
            }
            self.long_press = None;
            // The handler that runs is the current one for the area, not the one
            // from when the button went down, which may be out of date.
            let pressed = self.pressed.take();
            if let (Some(pressed), Some(pointer)) = (pressed, self.pointer)
                && let Some(region) = self.display.hit_test(pointer)
                && same_area(pressed, region.rect)
            {
                let on_click = Rc::clone(&region.on_click);
                on_click();
            }
            return;
        }
        self.pressed = target;
        self.carrying = self.pointer.and_then(|pointer| {
            let region = self.display.drag_source_at(pointer)?;
            Some(Carrying {
                payload: Rc::clone(&region.payload),
                began: pointer,
                active: false,
                rect: region.rect,
                commands: region.commands.clone(),
            })
        });
        self.long_press = self.pointer.and_then(|pointer| {
            let region = self.display.long_press_at(pointer)?;
            Some((
                Instant::now() + std::time::Duration::from_millis(500),
                Rc::clone(&region.on_long_press),
                pointer,
            ))
        });
        let focus = self
            .pointer
            .and_then(|pointer| self.display.focus_at(pointer))
            .map(|region| region.focus.clone());
        self.set_focus(focus);
        let Some(pointer) = self.pointer else {
            return;
        };
        if let Some(region) = self.display.drag_at(pointer) {
            let (on_drag, origin) = (Rc::clone(&region.on_drag), region.origin);
            on_drag(Offset::new(pointer.x - origin.x, pointer.y - origin.y));
            self.dragging = Some((on_drag, origin, region.on_end.clone()));
        }
        let click_count = self.count_click(pointer);
        if let Some(region) = self.display.press_at(pointer) {
            let position = Offset::new(pointer.x - region.origin.x, pointer.y - region.origin.y);
            let on_press = Rc::clone(&region.on_press);
            on_press(PressEvent {
                position,
                click_count,
            });
        }
    }

    /// How many presses in a row this is: later ones close in time and place add up.
    fn count_click(&mut self, at: Offset) -> u32 {
        let now = Instant::now();
        let count = match self.last_press {
            Some((time, place, count))
                if now.duration_since(time) < std::time::Duration::from_millis(450)
                    && (place.x - at.x).abs() < 5.0
                    && (place.y - at.y).abs() < 5.0 =>
            {
                count % 3 + 1
            }
            _ => 1,
        };
        self.last_press = Some((now, at, count));
        count
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Rustle")
            // Hidden until the accessibility adapter exists.
            .with_visible(false)
            .with_inner_size(LogicalSize::new(1920.0, 1080.0))
            .with_min_inner_size(LogicalSize::new(360.0, 240.0));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => return self.fail(event_loop, error),
        };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(event_loop.owned_display_handle()),
        ));
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        let adapter = accesskit_winit::Adapter::with_event_loop_proxy(event_loop, &window, proxy);
        window.set_visible(true);
        self.set_dark(window.theme() != Some(WindowTheme::Light));
        rustle_ui::set_clipboard(Rc::new(input::SystemClipboard::new()));
        let size = window.inner_size();
        match Renderer::new(instance, window.clone(), (size.width, size.height)) {
            Ok(renderer) => {
                self.window = Some(WindowState {
                    adapter,
                    renderer,
                    window,
                });
                self.relayout();
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some((due, _, _)) = &self.long_press else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        if Instant::now() >= *due {
            if let Some((_, on_long_press, _)) = self.long_press.take() {
                // The press has become a long press, so it is no longer a click.
                self.pressed = None;
                on_long_press();
            }
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(*due));
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Wake => {}
            UserEvent::Accessibility(event) => self.accessibility_event(event.window_event),
        }
        if self.composition.recompose() {
            self.relayout();
        }
        self.apply_focus_request();
        self.request_redraw_for_frames();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::RedrawRequested) {
            self.run_frame();
        }
        let Some(state) = &mut self.window else {
            return;
        };
        if self.debug_input
            && !matches!(
                event,
                WindowEvent::RedrawRequested | WindowEvent::CursorMoved { .. }
            )
        {
            eprintln!("rustle input: window event {event:?}");
        }
        state.adapter.process_event(&state.window, &event);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                state.renderer.resize(size.width, size.height);
                self.relayout();
            }
            WindowEvent::ScaleFactorChanged { .. } => self.relayout(),
            WindowEvent::CursorMoved { position, .. } => {
                let scale_factor = state.window.scale_factor();
                let pointer = Offset::new(
                    (position.x / scale_factor) as f32,
                    (position.y / scale_factor) as f32,
                );
                self.pointer_moved(pointer);
            }
            WindowEvent::Touch(touch) => {
                let scale_factor = state.window.scale_factor();
                let at = Offset::new(
                    (touch.location.x / scale_factor) as f32,
                    (touch.location.y / scale_factor) as f32,
                );
                // A finger is a pointer that is only there while it touches.
                match touch.phase {
                    TouchPhase::Started => {
                        self.pointer_moved(at);
                        self.primary_button(true);
                    }
                    TouchPhase::Moved => self.pointer_moved(at),
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.pointer_moved(at);
                        self.primary_button(false);
                        self.pointer = None;
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scale_factor = state.window.scale_factor() as f32;
                // A line of a mouse wheel scrolls this many logical pixels.
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 48.0, y * 48.0),
                    MouseScrollDelta::PixelDelta(delta) => {
                        (delta.x as f32 / scale_factor, delta.y as f32 / scale_factor)
                    }
                };
                if let Some(pointer) = self.pointer {
                    for (vertical, amount) in [(true, dy), (false, dx)] {
                        if amount != 0.0
                            && let Some(region) = self.display.scroll_at(pointer, vertical)
                        {
                            region.state.scroll_by(-amount);
                        }
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer = None;
                state.window.request_redraw();
                self.update_following();
                self.update_hover();
            }
            WindowEvent::MouseInput {
                state: button_state,
                button: MouseButton::Left,
                ..
            } => self.primary_button(button_state == ElementState::Pressed),
            WindowEvent::DroppedFile(path) => {
                // The platform gives no position, so the file goes where the pointer last was.
                if let Some(pointer) = self.pointer
                    && let Some(region) = self.display.file_drop_at(pointer)
                {
                    let on_drop = Rc::clone(&region.on_drop);
                    on_drop(path);
                }
            }
            WindowEvent::PinchGesture { delta, .. } => {
                if let Some(pointer) = self.pointer
                    && let Some(region) = self.display.pinch_at(pointer)
                {
                    let on_pinch = Rc::clone(&region.on_pinch);
                    on_pinch(delta as f32);
                }
            }
            WindowEvent::ThemeChanged(theme) => self.set_dark(theme == WindowTheme::Dark),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = input::modifiers(modifiers.state());
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let event = input::key_event(&event, self.modifiers);
                if self.debug_input {
                    eprintln!("rustle input: key {event:?}");
                }
                if event.pressed
                    && let Some(text) = &event.text
                {
                    self.typed = Some((Instant::now(), text.clone()));
                }
                self.key_event(event);
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if self.debug_input {
                    eprintln!("rustle input: input method committed {text:?}");
                }
                // Some input methods deliver text a key press already typed.
                let repeated = self.typed.as_ref().is_some_and(|(when, typed)| {
                    *typed == text && when.elapsed() < std::time::Duration::from_millis(60)
                });
                if !repeated {
                    self.key_event(KeyEvent {
                        key: Key::Other,
                        text: Some(text),
                        modifiers: self.modifiers,
                        pressed: true,
                    });
                }
            }
            WindowEvent::RedrawRequested => {
                let scale_factor = state.window.scale_factor() as f32;
                let carried = carrying_overlay(&self.display, self.carrying.as_ref(), self.pointer);
                let feedback = if carried.is_some() {
                    None
                } else {
                    feedback(&self.display, self.pointer, &self.pressed)
                };
                let commands = match (feedback, carried) {
                    (None, None) => Cow::Borrowed(self.display.commands.as_slice()),
                    (feedback, carried) => {
                        let mut commands = self.display.commands.clone();
                        commands.extend(feedback);
                        commands.extend(carried.into_iter().flatten());
                        Cow::Owned(commands)
                    }
                };
                match state.renderer.render(&commands, scale_factor) {
                    Ok(true) => {}
                    Ok(false) => state.window.request_redraw(),
                    Err(error) => return self.fail(event_loop, error),
                }
                self.request_redraw_for_frames();
            }
            _ => {}
        }
        self.apply_focus_request();
    }
}

/// Identifies the clickable region under `pointer`, to tell when it changes.
fn hover_target(display: &DisplayList, pointer: Option<Offset>) -> Option<Rect> {
    Some(display.hit_test(pointer?)?.rect)
}

/// Whether two areas are the same one, laid out again: where they are and how big.
fn same_area(a: Rect, b: Rect) -> bool {
    (a.origin.x - b.origin.x).abs() < 0.5
        && (a.origin.y - b.origin.y).abs() < 0.5
        && (a.size.width - b.size.width).abs() < 0.5
        && (a.size.height - b.size.height).abs() < 0.5
}

/// What is drawn on top while something is carried: the drop target under the
/// pointer lit up, the place it came from dimmed, and a lifted, translucent copy
/// of it following the pointer.
fn carrying_overlay(
    display: &DisplayList,
    carrying: Option<&Carrying>,
    pointer: Option<Offset>,
) -> Option<Vec<DrawCommand>> {
    let carrying = carrying.filter(|carrying| carrying.active)?;
    let pointer = pointer?;
    let mut out = Vec::new();
    // Every place it can be dropped is outlined; the one it would land on is filled.
    let over = display.drop_target_at(pointer).map(|target| target.rect);
    for target in &display.drop_targets {
        let hot = over.is_some_and(|rect| same_area(rect, target.rect));
        out.push(DrawCommand::Rect {
            rect: target.rect,
            color: Color::rgba(59, 130, 246, if hot { 40 } else { 0 }),
            corner_radius: 10.0,
            border: Some(Border {
                width: if hot { 2.0 } else { 1.0 },
                color: Color::rgba(59, 130, 246, if hot { 255 } else { 110 }),
            }),
        });
    }
    out.push(DrawCommand::Rect {
        rect: carrying.rect,
        color: Color::rgba(128, 128, 128, 70),
        corner_radius: 8.0,
        border: Some(Border {
            width: 1.0,
            color: Color::rgba(128, 128, 128, 140),
        }),
    });
    let (dx, dy) = (pointer.x - carrying.began.x, pointer.y - carrying.began.y);
    let lifted = Rect::new(
        Offset::new(carrying.rect.origin.x + dx, carrying.rect.origin.y + dy),
        carrying.rect.size,
    );
    out.push(DrawCommand::Shadow {
        rect: lifted,
        corner_radius: 8.0,
        blur: 18.0,
        color: Color::rgba(0, 0, 0, 90),
    });
    out.push(DrawCommand::PushLayer {
        rect: lifted,
        alpha: 0.9,
        rotation: 0.03,
        scale: 1.04,
    });
    if let Some(commands) = display.commands.get(carrying.commands.clone()) {
        out.extend(
            commands
                .iter()
                .cloned()
                .map(|command| command.translated(dx, dy)),
        );
    }
    out.push(DrawCommand::PopLayer);
    Some(out)
}

/// The tint drawn over the clickable region under the pointer: lighter while
/// hovering, darker while pressed. Nothing is drawn if the button went down on
/// a different region.
fn feedback(
    display: &DisplayList,
    pointer: Option<Offset>,
    pressed: &Option<Rect>,
) -> Option<DrawCommand> {
    let region = display.hit_test(pointer?)?;
    if !region.feedback {
        return None;
    }
    let is_pressed = pressed.is_some_and(|pressed| same_area(pressed, region.rect));
    if pressed.is_some() && !is_pressed {
        return None;
    }
    Some(DrawCommand::Rect {
        rect: region.rect,
        color: if is_pressed {
            Color::rgba(0, 0, 0, 70)
        } else {
            Color::rgba(255, 255, 255, 30)
        },
        corner_radius: region.corner_radius,
        border: None,
    })
}

/// Composes `root` and draws it once, with no window, into a PNG at `path`.
///
/// The picture is `width` by `height` pixels and uses the dark or the light
/// appearance. State, effects and tasks that wait on frames are not run
/// beyond the first composition. This is the way to see a composable without
/// opening a window, from a build script, a tool or another program.
pub fn preview(
    root: impl FnMut() + 'static,
    (width, height): (u32, u32),
    dark: bool,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    snapshot(root, dark, (width, height), path)
}

/// Composes `root` and draws it once, off screen, into a PNG at `path`.
fn snapshot(
    mut root: impl FnMut() + 'static,
    dark: bool,
    (width, height): (u32, u32),
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut composition = rustle_ui::composition(move || {
        rustle_foundation::LOCAL_SYSTEM_DARK.provide(dark, &mut root);
    });
    let mut renderer = Renderer::offscreen(width, height)?;
    composition.compose();
    let constraints = Constraints::loose(Size::new(width as f32, height as f32));
    // Laying out can change state, such as scrolling a caret into view, which
    // composes and lays out again, as the event loop does. A few rounds settle it.
    let settle = |composition: &mut Composition,
                  renderer: &mut Renderer|
     -> Result<DisplayList, Box<dyn std::error::Error>> {
        let mut display = None;
        for _ in 0..8 {
            let tree = composition.output().ok_or("nothing was composed")?;
            display = Some(rustle_ui::display_list(tree, constraints, renderer));
            if !composition.recompose() {
                break;
            }
        }
        display.ok_or_else(|| "nothing was laid out".into())
    };
    let mut display = settle(&mut composition, &mut renderer)?;

    // Replays input against the laid-out UI, step by step, as the event loop
    // would handle it: RUSTLE_SNAPSHOT_INPUT="click:x,y;move:x,y;type:text;key:Backspace".
    let mut focused: Option<Focus> = None;
    let mut clock = std::time::Duration::ZERO;
    let steps = std::env::var("RUSTLE_SNAPSHOT_INPUT").unwrap_or_default();
    for step in steps.split(';').filter(|step| !step.is_empty()) {
        let (kind, argument) = step.split_once(':').unwrap_or((step, ""));
        match kind {
            "click" => {
                let (x, y) = argument.split_once(',').ok_or("click needs x,y")?;
                let at = Offset::new(x.trim().parse()?, y.trim().parse()?);
                let next = display.focus_at(at).map(|region| region.focus.clone());
                if focused.as_ref().map(Focus::id) != next.as_ref().map(Focus::id) {
                    if let Some(old) = &focused {
                        old.set_focused(false);
                    }
                    if let Some(new) = &next {
                        new.set_focused(true);
                    }
                    focused = next;
                }
                if let Some(region) = display.press_at(at) {
                    let position = Offset::new(at.x - region.origin.x, at.y - region.origin.y);
                    (Rc::clone(&region.on_press))(PressEvent {
                        position,
                        click_count: 1,
                    });
                }
                if let Some(region) = display.hit_test(at) {
                    (Rc::clone(&region.on_click))();
                }
            }
            "move" => {
                // Puts the pointer at x,y: hover and pointer-move handlers hear about it.
                let (x, y) = argument.split_once(',').ok_or("move needs x,y")?;
                let at = Offset::new(x.trim().parse()?, y.trim().parse()?);
                for region in display.hovers_at(at) {
                    (Rc::clone(&region.on_hover))(true);
                }
                for region in &display.pointer_moves {
                    let rect = region.rect;
                    if at.x >= rect.origin.x
                        && at.x < rect.origin.x + rect.size.width
                        && at.y >= rect.origin.y
                        && at.y < rect.origin.y + rect.size.height
                    {
                        let local = Offset::new(at.x - region.origin.x, at.y - region.origin.y);
                        (Rc::clone(&region.on_move))(Some(local));
                    }
                }
            }
            "type" | "key" => {
                let key = if kind == "type" {
                    Key::Character(argument.to_string())
                } else {
                    match argument {
                        "Backspace" => Key::Backspace,
                        "Delete" => Key::Delete,
                        "Enter" => Key::Enter,
                        "Tab" => Key::Tab,
                        "Left" => Key::ArrowLeft,
                        "Right" => Key::ArrowRight,
                        "Up" => Key::ArrowUp,
                        "Down" => Key::ArrowDown,
                        "Home" => Key::Home,
                        "End" => Key::End,
                        other => return Err(format!("unknown key {other}").into()),
                    }
                };
                let handler = focused.as_ref().and_then(|focus| {
                    display
                        .focusables
                        .iter()
                        .find(|region| region.focus.id() == focus.id())
                        .map(|region| Rc::clone(&region.on_key))
                });
                if let Some(handler) = handler {
                    handler(&KeyEvent {
                        key,
                        text: (kind == "type").then(|| argument.to_string()),
                        modifiers: Modifiers::default(),
                        pressed: true,
                    });
                } else {
                    eprintln!("rustle: no focused area to receive {step}");
                }
            }
            other => return Err(format!("unknown input step {other}").into()),
        }
        // Let animations and tasks that wait on frames run to their end: a second of frames.
        for _ in 0..60 {
            clock += std::time::Duration::from_millis(16);
            composition.run_frame(clock);
            composition.recompose();
        }
        display = settle(&mut composition, &mut renderer)?;
    }
    let (width, height, pixels) = renderer.snapshot(&display.commands, 1.0)?;
    let image = ImageData::from_rgba(width, height, pixels).ok_or("bad snapshot size")?;
    std::fs::write(path, image.encode_png()?)?;
    Ok(())
}

/// Opens `url` in the system's default browser or handler, without waiting for it.
///
/// Only `http`, `https` and `mailto` URLs without control characters or spaces are
/// opened; anything else is ignored. Returns whether the URL was handed to the system.
pub fn open_url(url: &str) -> bool {
    let allowed = ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| url.to_ascii_lowercase().starts_with(scheme));
    if !allowed || url.chars().any(|c| c.is_control() || c == ' ') {
        return false;
    }
    #[cfg(target_os = "macos")]
    let command = ("open", vec![url.to_string()]);
    // Not through cmd.exe, which would read `&` and `|` in the URL as commands.
    #[cfg(target_os = "windows")]
    let command = (
        "rundll32",
        vec!["url.dll,FileProtocolHandler".to_string(), url.to_string()],
    );
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let command = ("xdg-open", vec![url.to_string()]);
    process::Command::new(command.0)
        .args(&command.1)
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .spawn()
        .is_ok()
}
