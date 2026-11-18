//! Design-system-agnostic building blocks: layouts and basic elements.
//!
//! Everything here is a composable built on the primitives of `rustle-ui`.
//! Each element has a plain form and a `_with` form taking a [`Modifier`].

mod constraint;
mod flow;
mod layouts;
mod lazy;
mod linear;
mod locale;
mod selection;
mod sized;
mod text;
mod text_field;
mod viewport;

#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod lazy_motion_tests;

pub use constraint::{
    Anchor, BarrierDirection, ChainStyle, ChildConstraints, ConstraintSet, Dimension,
    GuidePosition, Ref, constraint_layout,
};
pub use flow::{FlowOptions, flow_column, flow_column_with, flow_row, flow_row_with};
pub use layouts::{backdrop, floating, floating_above, reveal, with_constraints};
pub use lazy::{
    ItemAlign, LazyContent, LazyListState, lazy_column, lazy_horizontal_list,
    lazy_horizontal_list_keyed, lazy_horizontal_list_with, lazy_horizontal_staggered_grid,
    lazy_horizontal_sticky_list, lazy_list, lazy_list_keyed, lazy_list_with, lazy_row,
    lazy_sticky_list, lazy_vertical_grid, lazy_vertical_grid_spanned, lazy_vertical_staggered_grid,
    remember_lazy_list_state,
};
pub use linear::{
    Alignment, Arrangement, column, column_arranged, column_with, row, row_arranged, row_with,
    spacer, stack, stack_aligned, stack_with,
};
pub use locale::{
    LOCAL_LOCALE, LOCAL_STRING_RESOURCES, Locale, StringResources, provide_locale,
    provide_string_resources, string_resource, string_resource_with,
};
pub use sized::sized_box;
pub use text::{
    LOCAL_SYSTEM_DARK, LOCAL_TEXT_STYLE, image, is_system_in_dark_theme, mask_image, rich_text,
    rich_text_with, text, text_styled, text_with,
};
pub use viewport::{viewport, viewport_with};

pub use rustle_ui::Modifier;
pub use rustle_ui::{
    AnchoredDraggableState, Axis, DraggableState, NestedScrollConnection, PointerId, PointerInput,
    PointerInputPhase, PointerKind, PointerSample, ScrollableState, TransformableState,
    VelocityTracker, remember_anchored_draggable_state, remember_draggable_state,
    remember_scrollable_state, remember_transformable_state,
};
pub use rustle_ui::{
    BoxAlignment, Canvas, ContentScale, Focus, Key, KeyCode, KeyEvent, KeyLocation,
    LOCAL_LAYOUT_DIRECTION, LayoutDirection, LiveRegion, Modifiers, PhysicalKey, PointerButton,
    PointerEvent, PointerPhase, Preedit, PressEvent, Role, ScrollDirection, ScrollState,
    ScrollbarStyle, ScrollbarVisibility, Semantics, canvas, is_rtl, remember_focus,
    remember_scroll_state,
};
pub use rustle_ui::{
    IntrinsicSize, Measurable, Placement, SubcomposeResult, SubcomposeScope, subcompose_layout,
};
pub use selection::{
    LOCAL_TEXT_SELECTION_COLORS, SelectionPoint, SelectionState, TextSelectionColors,
    remember_selection_state, selection_container, selection_container_with,
};
pub use text_field::{
    TextFieldOptions, TextFieldState, TextFieldValue, basic_text_area, basic_text_area_no_wrap,
    basic_text_field, basic_text_field_in, remember_text_field_state, selectable_text,
};
