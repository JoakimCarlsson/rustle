//! Rustle: a tiny declarative UI framework.

use std::process;

mod accessibility;
#[cfg(test)]
mod accessibility_tests;
mod app;
mod contact;
mod drag_and_drop;
mod embedded;
mod file_drops;
mod focus;
mod frame_timing;
mod handler;
mod host;
mod input;
mod interface;
mod overlay;
mod pointer_events;
mod pointing;
mod preview;
mod saved;
mod shadow;
#[cfg(test)]
mod testing;
mod window;
mod window_info;
mod window_state;
#[cfg(test)]
mod window_tests;
mod windows;

pub use app::{
    external_waker, run, run_with, set_close_handler, set_external_handler, set_pixel_rounding,
};
pub use embedded::Embedded;
pub use frame_timing::{FrameTiming, UpdateTiming, set_frame_observer};
pub use host::Host;
pub use preview::{
    PreviewOptions, preview, preview_pixels, preview_pixels_composing, preview_window,
};
pub use shadow::{ShadowLayer, WindowShadow};
pub use window::{ResizeEdge, WindowHandle, WindowOptions, WindowTheme, current_window, window};
pub use window_info::{LOCAL_WINDOW_INFO, WindowInfo};
pub use window_state::{WindowPlacement, WindowPosition, WindowState, remember_window_state};

pub use rustle_animation::{
    Animatable, AnimationSpec, DecaySpec, Easing, Keyframe, Lerp, RepeatMode, Transition,
    animate_as_state, animate_color_as_state, animate_decay, animate_float_as_state, animate_value,
    infinite_float, infinite_float_repeating, infinite_repeatable, keyframes, paths_compatible,
    remember_animatable, repeatable, snap, spring, tween, update_transition,
};
pub use rustle_animation::{InfiniteTransition, remember_infinite_transition};
pub use rustle_charts::{
    ChartStyle, Series, area_chart, bar_chart, bar_hover, chart_hover, donut_chart, line_chart,
    qr_code, sparkline,
};
pub use rustle_foundation::svg_image;
pub use rustle_foundation::{
    Alignment, Arrangement, BoxAlignment, Canvas, ContentScale, ItemAlign,
    LOCAL_PINNABLE_CONTAINER, LOCAL_SCROLLBAR_STYLE, LOCAL_TEXT_SELECTION_COLORS, LOCAL_TEXT_STYLE,
    LazyContent, LazyListItemInfo, LazyListLayoutInfo, LazyListState, Modifier, PinnableContainer,
    PinnedHandle, ScrollState, ScrollbarStyle, ScrollbarVisibility, TextSelectionColors, backdrop,
    canvas, column, column_arranged, column_with, floating, floating_above, flow_row, image,
    lazy_column, lazy_list, lazy_list_keyed, lazy_row, lazy_vertical_grid, mask_image,
    remember_lazy_list_state, reveal, rich_text, rich_text_with, row, row_arranged, row_with,
    spacer, stack, stack_aligned, stack_propagating, stack_with, text, text_styled, text_with,
    viewport, viewport_with,
};
pub use rustle_foundation::{
    AlignmentOffsetPositionProvider, BasicTooltipState, DropdownMenuPositionProvider,
    POPUP_Z_INDEX, PlainTooltipPositionProvider, PopupPositionProvider, PopupProperties,
    PositionAtPoint, basic_tooltip_box, popup_position_provider_at_position, popup_with,
    remember_basic_tooltip_state, tooltip_area,
};
pub use rustle_foundation::{
    Anchor, BarrierDirection, ChainStyle, ChildConstraints, ConstraintSet, Dimension, FlowOptions,
    GuidePosition, Ref, constraint_layout, flow_column, flow_column_with, flow_row_with,
    with_constraints,
};
pub use rustle_foundation::{
    AnchoredDraggableState, Axis, DraggableState, NestedScrollConnection, PointerId, PointerInput,
    PointerInputPhase, PointerKind, PointerSample, ScrollableState, TransformableState,
    VelocityTracker, remember_anchored_draggable_state, remember_draggable_state,
    remember_scrollable_state, remember_transformable_state,
};
pub use rustle_foundation::{
    BasicTextFieldOptions, ByValue, ChangeList, DEFAULT_OBFUSCATION_CHARACTER, ImeAction,
    InputTransformation, KeyboardActionHandler, KeyboardOptions, KeyboardType, MaxLength,
    OutputTransformation, TextFieldBuffer, TextFieldDecorator, TextFieldLineLimits,
    TextObfuscationMode, TextRange, Then, UndoState, basic_secure_text_field,
    basic_text_field_with,
};
pub use rustle_foundation::{
    CARET_WIDTH, CaretShape, MarkPaint, MarkedTextOptions, TextMark, draw_text_caret,
    draw_text_underline, marked_text, text_ellipsized,
};
pub use rustle_foundation::{
    Focus, Key, KeyCode, KeyEvent, KeyLocation, LOCAL_LAYOUT_DIRECTION, LOCAL_SYSTEM_DARK,
    LayoutDirection, Modifiers, PhysicalKey, PointerButton, PointerEvent, PointerPhase, Preedit,
    PressEvent, Role, Semantics, TextFieldOptions, TextFieldState, TextFieldValue, basic_text_area,
    basic_text_field, basic_text_field_in, is_rtl, is_system_in_dark_theme, remember_focus,
    remember_scroll_state, remember_text_field_state, selectable_text,
};
pub use rustle_foundation::{
    SelectionPoint, SelectionState, basic_text_area_no_wrap, remember_selection_state,
    selection_container, selection_container_with,
};
pub use rustle_foundation::{TextOverflow, basic_text, disable_selection};
pub use rustle_foundation::{
    lazy_horizontal_list, lazy_horizontal_list_keyed, lazy_horizontal_list_with,
    lazy_horizontal_staggered_grid, lazy_list_with, lazy_sticky_list, lazy_vertical_grid_spanned,
    lazy_vertical_staggered_grid,
};
pub use rustle_geometry::{Offset, Rect, Size};
pub use rustle_graphics::Shadow;
pub use rustle_graphics::SvgPainter;
pub use rustle_graphics::TextWrap;
pub use rustle_graphics::{
    BlendMode, Color, ColorFilter, DrawCommand, ImageData, ImageKey, LayerEffect, ShaderBrush,
    TextAlign,
};
pub use rustle_graphics::{FillRule, Path, PathCommand};
pub use rustle_macros::composable;
pub use rustle_navigation::{NavController, nav_host, remember_nav_controller};
pub use rustle_runtime::{
    CompositionLocal, State, StateList, StateMap, TaskScope, derived_state_of, disposable_effect,
    group, key, launched_effect, memo, produce_state, remember, remember_saveable,
    remember_state_list, remember_state_map, remember_updated_state, side_effect,
};
pub use rustle_runtime::{Saver, Subcomposition, is_composing, mutable_state, set_saver};
pub use rustle_text::{
    Affinity, Caret, Cluster, FontFamily, FontWeight, LineLayout, TextLayout, TextSpan, TextStyle,
    WrappedLine, ellipsize, font_epoch, font_families, layout_spans, layout_text,
    monospace_font_families, register_font, wrap_at_spaces,
};
pub use rustle_ui::{Children, Layout, LayoutResult, Node, collect, compose, emit, layout};
pub use rustle_ui::{
    Clipboard, ClipboardImage, CursorIcon, DisplayOptions, Interaction, InteractionColors,
    IntrinsicSize, Measurable, PixelRounding, Placement, SubcomposeResult, SubcomposeScope,
    TaggedNode, clipboard, restartable, set_clipboard, skippable, subcompose_layout,
};
pub use rustle_ui::{
    Density, DragInteraction, FocusInteraction, HoverInteraction, Indication,
    IndicationNodeFactory, InteractionId, InteractionSource, LOCAL_DENSITY, LOCAL_INDICATION,
    MutableInteractionSource, PlatformTextInputMethodRequest, PlatformTextInputSession,
    PointerButtons, PointerEventPass, PointerEventType, PointerInputChange, PressInteraction,
    interaction_source, pointer, remember_mutable_interaction_source,
    remember_platform_text_input_session,
};
pub use rustle_ui::{
    DragAndDropEvent, DragAndDropTarget, DragAndDropTransferData, DragData,
    LOCAL_VIEW_CONFIGURATION, ViewConfiguration,
};
pub use rustle_unit::{Constraints, Dp, dp};
pub use rustle_widgets::svg_icon;
pub use rustle_widgets::{
    BadgeStyle, ButtonColors, ButtonDefaults, ButtonStyle, ColorScheme, ContentTransform,
    CornerSizes, EnterTransition, ExitTransition, Icon, MenuItem, PagerState, RippleConfiguration,
    RippleModifier, Shape, ShapeModifier, Shapes, SharedElements, Typography,
    absolute_tonal_elevation, alert_dialog, animate_content_size, animate_scroll_to,
    animated_content, animated_visibility, avatar, badge, bottom_sheet, button, button_custom,
    button_with, calendar, card, card_header, checkbox, chip, circular_progress_indicator,
    circular_progress_indicator_indeterminate, color_scheme, content_color, content_color_for,
    crossfade, dark_color_scheme, dashed_spinner, days_in_month, dialog, divider, dropdown_menu,
    dropdown_menu_item, first_weekday, floating_action_button, horizontal_pager, icon, icon_button,
    light_color_scheme, linear_progress_indicator, list_item, menu, muted, muted_sized,
    navigation_bar, outlined_button, page_indicator, phosphor, pill, popup, progress_rings,
    provide_content_color, provide_ripple_configuration, radio_button, range_slider,
    remember_pager_state, remember_shared_elements, ripple_configuration, scaffold,
    segmented_control, select, select_field, separator, shapes, shared_element, slider, snackbar,
    styled_button, surface, surface_elevated, surface_with, swipe_to_dismiss, switch, tab_row,
    table_cell, table_row, tabs_list, text_area, text_button, text_field, text_field_focused,
    theme, title, tooltip, top_app_bar, typography,
};
pub use rustle_widgets::{
    BorderStroke, CardColors, CardDefaults, CheckboxColors, CheckboxDefaults, DividerDefaults,
    SwitchColors, SwitchDefaults, ToggleableState, checkbox_with, horizontal_divider,
    outlined_card, surface_custom, switch_with, tri_state_checkbox, vertical_divider,
};
pub use rustle_widgets::{
    DialogProperties, ProgressIndicatorDefaults, SegmentedButtonColors, SegmentedButtonDefaults,
    dialog_with, linear_progress_indicator_with, segmented_button,
    single_choice_segmented_button_row,
};
pub use rustle_widgets::{
    ShadowModifier, circular_progress_indicator_with, circular_progress_indicator_with_progress,
};
pub use rustle_widgets::{
    TooltipDefaults, TooltipState, dropdown_menu_anchored, remember_tooltip_state, tooltip_box,
};

pub mod prelude {
    pub use crate::{
        Alignment, AnchoredDraggableState, Animatable, AnimationSpec, Arrangement, Axis,
        BadgeStyle, BoxAlignment, ButtonColors, ButtonDefaults, ButtonStyle, Canvas, ChartStyle,
        Color, ColorScheme, CompositionLocal, ContentScale, ContentTransform, CornerSizes,
        CursorIcon, DecaySpec, Dp, DraggableState, Easing, EnterTransition, ExitTransition, Focus,
        FontFamily, FontWeight, Icon, ImageData, ImageKey, Keyframe, LOCAL_LAYOUT_DIRECTION,
        LOCAL_TEXT_SELECTION_COLORS, LOCAL_TEXT_STYLE, LayoutDirection, LazyListState, MenuItem,
        Modifier, NavController, NestedScrollConnection, PagerState, PointerInput,
        PointerInputPhase, PointerKind, RepeatMode, RippleConfiguration, RippleModifier, Role,
        ScrollState, ScrollableState, Semantics, Series, Shape, ShapeModifier, Shapes,
        SharedElements, State, StateList, StateMap, TaskScope, TextAlign, TextFieldState,
        TextSelectionColors, TextSpan, TextStyle, TransformableState, Typography, WindowOptions,
        absolute_tonal_elevation, alert_dialog, animate_color_as_state, animate_content_size,
        animate_float_as_state, animate_value, animated_content, animated_visibility, area_chart,
        avatar, backdrop, badge, bar_chart, bar_hover, bottom_sheet, button, button_custom,
        button_with, calendar, canvas, card, card_header, chart_hover, checkbox, chip,
        circular_progress_indicator, circular_progress_indicator_indeterminate, color_scheme,
        column, column_arranged, column_with, composable, content_color, content_color_for,
        crossfade, current_window, dark_color_scheme, days_in_month, derived_state_of, dialog,
        disposable_effect, divider, donut_chart, dp, dropdown_menu, dropdown_menu_item,
        first_weekday, floating, floating_above, floating_action_button, flow_row,
        horizontal_pager, icon, icon_button, image, infinite_float, infinite_float_repeating,
        infinite_repeatable, is_rtl, is_system_in_dark_theme, key, keyframes, launched_effect,
        lazy_column, lazy_list, lazy_row, lazy_vertical_grid, light_color_scheme, line_chart,
        linear_progress_indicator, list_item, mask_image, menu, muted, muted_sized, nav_host,
        navigation_bar, outlined_button, page_indicator, phosphor, pill, popup, produce_state,
        provide_content_color, provide_ripple_configuration, qr_code, radio_button, range_slider,
        remember, remember_anchored_draggable_state, remember_animatable, remember_draggable_state,
        remember_focus, remember_lazy_list_state, remember_nav_controller, remember_pager_state,
        remember_saveable, remember_scroll_state, remember_scrollable_state,
        remember_shared_elements, remember_state_list, remember_state_map,
        remember_text_field_state, remember_transformable_state, remember_updated_state,
        repeatable, reveal, rich_text, rich_text_with, ripple_configuration, row, row_arranged,
        row_with, scaffold, segmented_control, select, select_field, selectable_text,
        selection_container, separator, shapes, shared_element, side_effect, slider, snackbar,
        snap, spacer, sparkline, spring, stack, stack_aligned, stack_with, styled_button, surface,
        surface_elevated, surface_with, swipe_to_dismiss, switch, tab_row, table_cell, table_row,
        tabs_list, text, text_area, text_button, text_field, text_field_focused, text_styled,
        text_with, theme, title, tooltip, top_app_bar, tween, typography, update_transition,
        viewport, window,
    };
    pub use crate::{
        BlendMode, ColorFilter, FlowOptions, IntrinsicSize, LayerEffect, LazyContent,
        LazyListItemInfo, LazyListLayoutInfo, ShaderBrush, flow_column, flow_column_with,
        flow_row_with, lazy_horizontal_list, lazy_horizontal_list_keyed, lazy_horizontal_list_with,
        lazy_horizontal_staggered_grid, lazy_list_keyed, lazy_list_with, lazy_sticky_list,
        lazy_vertical_grid_spanned, lazy_vertical_staggered_grid, with_constraints,
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
