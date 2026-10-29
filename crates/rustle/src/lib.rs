//! Rustle: a tiny declarative UI framework.

use std::process;

mod accessibility;
mod app;
mod focus;
mod handler;
mod input;
mod overlay;
mod pointer;
mod preview;
mod saved;

pub use app::run;
pub use preview::preview;

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
    navigation_bar, outlined_button, page_indicator, phosphor, pill, popup, provide_theme,
    radio_button, range_slider, remember_pager_state, scaffold, segmented_control, select,
    select_field, separator, slider, snackbar, styled_button, surface, surface_with,
    swipe_to_dismiss, switch, system_theme, tab_row, table_cell, table_row, tabs_list, text_area,
    text_button, text_field, text_field_focused, theme, title, tooltip, top_app_bar,
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
        muted_sized, nav_host, navigation_bar, outlined_button, page_indicator, phosphor, pill,
        popup, produce_state, provide_theme, qr_code, radio_button, range_slider, remember,
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
