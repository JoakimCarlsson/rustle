//! Design-system-agnostic building blocks: layouts and basic elements.
//!
//! Everything here is a composable built on the primitives of `rustle-ui`.
//! Each element has a plain form and a `_with` form taking a [`Modifier`].

mod layouts;
mod lazy;
mod linear;
mod selection;
mod text;
mod text_field;
mod viewport;

pub use layouts::{backdrop, floating, flow_row, reveal};
pub use lazy::{
    ItemAlign, LazyListState, lazy_column, lazy_list, lazy_list_keyed, lazy_row,
    lazy_vertical_grid, remember_lazy_list_state,
};
pub use linear::{
    Alignment, Arrangement, column, column_arranged, column_with, row, row_arranged, row_with,
    spacer, stack, stack_aligned, stack_with,
};
pub use text::{
    LOCAL_SYSTEM_DARK, LOCAL_TEXT_STYLE, image, is_system_in_dark_theme, rich_text, rich_text_with,
    text, text_styled, text_with,
};
pub use viewport::{viewport, viewport_with};

pub use rustle_ui::Modifier;
pub use rustle_ui::{
    BoxAlignment, Canvas, ContentScale, Focus, Key, KeyCode, KeyEvent, KeyLocation,
    LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifiers, PhysicalKey, PointerButton, PointerEvent,
    PointerPhase, Preedit, PressEvent, Role, ScrollState, ScrollbarStyle, ScrollbarVisibility,
    Semantics, canvas, is_rtl, remember_focus, remember_scroll_state,
};
pub use selection::{
    SelectionPoint, SelectionState, remember_selection_state, selection_container,
    selection_container_with,
};
pub use text_field::{
    TextFieldOptions, TextFieldState, TextFieldValue, basic_text_area, basic_text_area_no_wrap,
    basic_text_field, basic_text_field_in, remember_text_field_state, selectable_text,
};
