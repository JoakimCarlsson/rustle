//! Styled, ready-made components such as buttons and surfaces, with their look.
//!
//! Built only from `rustle-foundation`. Colors and shapes come from the
//! current [`Theme`], which a subtree can replace with [`provide_theme`].

mod buttons;
mod controls;
mod data;
mod dialogs;
mod display;
mod feedback;
mod icons;
mod inputs;
mod layout;
mod menus;
mod motion;
mod navigation;
mod progress;
mod theme;
mod transitions;

#[cfg(test)]
mod scaffold_tests;
#[cfg(test)]
mod transition_tests;

pub use buttons::{
    ButtonStyle, button, button_with, floating_action_button, icon_button, outlined_button,
    styled_button, text_button,
};
pub use controls::{checkbox, radio_button, range_slider, slider, switch};
pub use data::{calendar, days_in_month, first_weekday, table_cell, table_row};
pub use dialogs::{alert_dialog, bottom_sheet, dialog, popup};
pub use display::{BadgeStyle, avatar, badge, chip, pill};
pub use feedback::{snackbar, tooltip};
pub use icons::icon;
pub use inputs::{select_field, text_area, text_field, text_field_focused};
pub use layout::{
    ScaffoldPadding, ScaffoldSlots, card, card_header, divider, list_item, muted, muted_sized,
    scaffold, scaffold_with, separator, surface, surface_with, title, top_app_bar,
};
pub use menus::{MenuItem, dropdown_menu, dropdown_menu_at, dropdown_menu_item, menu, select};
pub use motion::{
    PagerState, animate_scroll_to, animated_visibility, crossfade, horizontal_pager,
    page_indicator, remember_pager_state, swipe_to_dismiss,
};
pub use navigation::{navigation_bar, segmented_control, tab_row, tabs_list};
pub use progress::{
    circular_progress_indicator, circular_progress_indicator_indeterminate,
    linear_progress_indicator,
};
pub use rustle_icons::{Icon, phosphor};
pub use theme::{Theme, provide_theme, system_theme, theme};
pub use transitions::{
    ContentTransform, EnterTransition, ExitTransition, SharedElements, animate_content_size,
    animated_content, remember_shared_elements, shared_element,
};
