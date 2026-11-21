//! Popups and tooltips: Compose's `Popup(popupPositionProvider, onDismissRequest, properties)`,
//! `BasicTooltipBox` with its state, and Compose Desktop's `TooltipArea`.

use std::rc::Rc;
use std::time::Duration;

use rustle_geometry::{Offset, Size};
use rustle_runtime::{State, group, launched_effect, remember};
use rustle_ui::{
    Children, Layout, LayoutResult, Modifier, PopupPositionProvider, PopupProperties, layout,
};
use rustle_unit::Constraints;

use crate::stack_with;

/// The layer popups are drawn in: over everything else in the window.
pub const POPUP_Z_INDEX: i32 = 1 << 20;

/// Measures its children without limits and takes no room itself, nor asks for any when
/// asked its intrinsic size, so a popup leaves the layout it is declared in as it was.
struct PopupLayout;

impl Layout for PopupLayout {
    /// Each child as large as it likes, at the popup's corner; no room taken.
    fn measure(&self, children: &mut Children, _: Constraints) -> LayoutResult {
        let unbounded = Constraints {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
        };
        for index in 0..children.len() {
            children.measure(index, unbounded);
        }
        LayoutResult {
            size: Size::default(),
            positions: vec![Offset::default(); children.len()],
        }
    }

    /// Nothing: a popup takes no room, so it asks nothing of the layout around it, as
    /// Compose's `Popup` is an empty layout where it is declared.
    fn min_intrinsic_width(&self, _: &mut Children, _: f32) -> f32 {
        0.0
    }

    /// Nothing, as [`min_intrinsic_width`](Self::min_intrinsic_width) says.
    fn max_intrinsic_width(&self, _: &mut Children, _: f32) -> f32 {
        0.0
    }

    /// Nothing, as [`min_intrinsic_width`](Self::min_intrinsic_width) says.
    fn min_intrinsic_height(&self, _: &mut Children, _: f32) -> f32 {
        0.0
    }

    /// Nothing, as [`min_intrinsic_width`](Self::min_intrinsic_width) says.
    fn max_intrinsic_height(&self, _: &mut Children, _: f32) -> f32 {
        0.0
    }
}

/// Emits `content` as a popup: over everything else in the window, not cut by the clips
/// around where it is declared, where `position_provider` puts it against the bounds of the
/// layout it is declared in (its anchor) and the window, taking no room there. Compose's
/// `Popup(popupPositionProvider, onDismissRequest, properties, content)`.
///
/// The popup takes the presses that land on it. A press outside it calls
/// `on_dismiss_request` when `properties.dismiss_on_click_outside`, and goes on to what it
/// landed on unless `properties.focusable`; a focusable popup takes it, and Escape calls
/// `on_dismiss_request` too (with `dismiss_on_back_press`) before anything else sees the key. Hide the popup in
/// `on_dismiss_request` by not emitting it any more. The content is measured without limits.
#[track_caller]
pub fn popup_with(
    position_provider: impl PopupPositionProvider + 'static,
    on_dismiss_request: Option<Rc<dyn Fn()>>,
    properties: PopupProperties,
    content: impl FnOnce(),
) {
    layout(
        Modifier::new()
            .z_index_unclipped(POPUP_Z_INDEX)
            .popup_placement(position_provider, properties, on_dismiss_request),
        PopupLayout,
        content,
    );
}

/// Whether a tooltip shows: Compose's `BasicTooltipState`. Clones share the same state.
#[derive(Clone)]
pub struct BasicTooltipState {
    /// Whether the tooltip is shown.
    visible: State<bool>,
    /// Whether the tooltip stays until it is dismissed rather than going on its own.
    persistent: bool,
}

impl BasicTooltipState {
    /// Whether the tooltip is shown: Compose's `isVisible`.
    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }

    /// Whether the tooltip stays until it is dismissed: Compose's `isPersistent`.
    pub fn is_persistent(&self) -> bool {
        self.persistent
    }

    /// Shows the tooltip: Compose's `show()`.
    pub fn show(&self) {
        self.visible.set(true);
    }

    /// Hides the tooltip: Compose's `dismiss()`.
    pub fn dismiss(&self) {
        self.visible.set(false);
    }
}

/// A tooltip state kept across compositions, shown at first when `initial_is_visible`:
/// Compose's `rememberBasicTooltipState(initialIsVisible, isPersistent)`.
#[track_caller]
pub fn remember_basic_tooltip_state(
    initial_is_visible: bool,
    is_persistent: bool,
) -> BasicTooltipState {
    BasicTooltipState {
        visible: remember(|| initial_is_visible),
        persistent: is_persistent,
    }
}

/// Emits `content` in a box with `modifier` and, while `state` is visible, `tooltip` as a
/// popup placed by `position_provider` against that box: Compose's
/// `BasicTooltipBox(positionProvider, tooltip, state, modifier, focusable, enableUserInput,
/// content)`. With `enable_user_input`, the pointer coming over the box shows the tooltip
/// and leaving it hides it, unless it is persistent; a press outside the tooltip dismisses
/// it. With `focusable`, Escape dismisses it.
#[track_caller]
pub fn basic_tooltip_box(
    position_provider: impl PopupPositionProvider + 'static,
    tooltip: impl FnOnce(),
    state: &BasicTooltipState,
    modifier: Modifier,
    focusable: bool,
    enable_user_input: bool,
    content: impl FnOnce(),
) {
    group(|| {
        let modifier = if enable_user_input {
            let hover = state.clone();
            modifier.on_hover(move |over| {
                if over {
                    hover.show();
                } else if !hover.is_persistent() {
                    hover.dismiss();
                }
            })
        } else {
            modifier
        };
        let shown = state.is_visible();
        let dismiss = state.clone();
        stack_with(modifier, || {
            content();
            if shown {
                popup_with(
                    position_provider,
                    Some(Rc::new(move || dismiss.dismiss())),
                    PopupProperties {
                        focusable,
                        ..PopupProperties::default()
                    },
                    tooltip,
                );
            }
        });
    });
}

/// Emits `content` in a box with `modifier` and, once the pointer has rested on the box for
/// `delay_millis` milliseconds, `tooltip` as a popup placed by `tooltip_placement` against
/// the box, until the pointer leaves it: Compose Desktop's
/// `TooltipArea(tooltip, modifier, delayMillis, tooltipPlacement, content)`. The delay is
/// waited on the frame clock; with no delay the tooltip shows in the frame the pointer came.
#[track_caller]
pub fn tooltip_area(
    tooltip: impl FnOnce(),
    modifier: Modifier,
    delay_millis: u64,
    tooltip_placement: impl PopupPositionProvider + 'static,
    content: impl FnOnce(),
) {
    group(|| {
        let state = remember_basic_tooltip_state(false, false);
        let hovered = remember(|| false);
        let over = hovered.get();
        let shown = state.clone();
        launched_effect(over, move |scope| async move {
            if !over {
                shown.dismiss();
                return;
            }
            if delay_millis > 0 {
                scope.delay(Duration::from_millis(delay_millis)).await;
            }
            shown.show();
        });
        let visible = over && (state.is_visible() || delay_millis == 0);
        let hover = hovered.clone();
        stack_with(modifier.on_hover(move |inside| hover.set(inside)), || {
            content();
            if visible {
                popup_with(tooltip_placement, None, PopupProperties::default(), tooltip);
            }
        });
    });
}
