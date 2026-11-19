//! Styled, ready-made components such as buttons and surfaces, with their look.
//!
//! Built only from `rustle-foundation`. Colors and shapes come from the
//! current [`theme`]: a [`ColorScheme`], [`Typography`] and [`Shapes`], which a
//! subtree can replace by nesting [`theme`].

mod buttons;
mod controls;
mod data;
mod dialogs;
mod display;
mod dynamic;
mod feedback;
mod icons;
mod inputs;
mod layout;
mod menus;
mod motion;
mod navigation;
mod progress;
mod ripple;
mod shape;
mod theme;
mod transitions;

#[cfg(test)]
mod scaffold_tests;
#[cfg(test)]
mod transition_tests;

pub use buttons::{
    ButtonColors, ButtonDefaults, ButtonStyle, button, button_custom, button_with,
    floating_action_button, icon_button, outlined_button, styled_button, text_button,
};
pub use controls::{
    CheckboxColors, CheckboxDefaults, SwitchColors, SwitchDefaults, ToggleableState, checkbox,
    checkbox_with, radio_button, range_slider, slider, switch, switch_with, tri_state_checkbox,
};
pub use data::{calendar, days_in_month, first_weekday, table_cell, table_row};
pub use dialogs::{DialogProperties, alert_dialog, bottom_sheet, dialog, dialog_with, popup};
pub use display::{BadgeStyle, avatar, badge, chip, pill};
pub use feedback::{
    TooltipDefaults, TooltipState, remember_tooltip_state, snackbar, tooltip, tooltip_box,
};
pub use icons::{icon, svg_icon};
pub use inputs::{select_field, text_area, text_field, text_field_focused};
pub use layout::{
    CardColors, CardDefaults, DividerDefaults, ScaffoldPadding, ScaffoldSlots, card, card_header,
    divider, horizontal_divider, list_item, muted, muted_sized, outlined_card, scaffold,
    scaffold_with, separator, surface, surface_custom, surface_elevated, surface_with, title,
    top_app_bar, vertical_divider,
};
pub use menus::{
    MenuItem, dropdown_menu, dropdown_menu_anchored, dropdown_menu_at, dropdown_menu_item, menu,
    select,
};
pub use motion::{
    PagerState, animate_scroll_to, animated_visibility, crossfade, horizontal_pager,
    page_indicator, remember_pager_state, swipe_to_dismiss,
};
pub use navigation::{
    SegmentedButtonColors, SegmentedButtonDefaults, navigation_bar, segmented_button,
    segmented_control, single_choice_segmented_button_row, tab_row, tabs_list,
};
pub use progress::{
    ProgressIndicatorDefaults, circular_progress_indicator,
    circular_progress_indicator_indeterminate, dashed_spinner, linear_progress_indicator,
    linear_progress_indicator_with, progress_rings,
};
pub use ripple::{
    RippleConfiguration, RippleModifier, provide_ripple_configuration, ripple_configuration,
};
pub use rustle_icons::{Icon, phosphor};
pub use shape::{BorderStroke, CornerSizes, Shape, ShapeModifier};
pub use theme::{
    ColorScheme, Shapes, Typography, absolute_tonal_elevation, color_scheme, content_color,
    content_color_for, dark_color_scheme, light_color_scheme, provide_content_color, shapes, theme,
    typography,
};
pub use transitions::{
    ContentTransform, EnterTransition, ExitTransition, SharedElements, animate_content_size,
    animated_content, remember_shared_elements, shared_element,
};
