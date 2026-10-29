//! Text entry and selection fields.

use rustle_foundation::{
    Alignment, Arrangement, Focus, LOCAL_TEXT_STYLE, Modifier, Role, Semantics, TextFieldOptions,
    TextFieldState, basic_text_field_in, remember_focus, row_arranged, stack_with, text, text_with,
};
use rustle_icons::phosphor;

use crate::icons::icon;
use crate::theme::{theme, with_text_color};

/// Emits an outlined single-line text field showing `placeholder` while it is empty.
///
/// Size it with `modifier`, for example `Modifier::new().width(320.0)`. The
/// outline takes the theme's primary color while the field has focus.
#[track_caller]
pub fn text_field(modifier: Modifier, state: &TextFieldState, placeholder: &str) {
    let focus = remember_focus();
    text_field_focused(modifier, state, placeholder, &focus);
}

/// Like [`text_field`], with the field's `focus` given, so the caller can ask for it.
#[track_caller]
pub fn text_field_focused(
    modifier: Modifier,
    state: &TextFieldState,
    placeholder: &str,
    focus: &Focus,
) {
    let theme = theme();
    let outline = if focus.is_focused() {
        theme.on_surface_muted
    } else {
        theme.outline
    };
    let empty = state.text().is_empty();
    stack_with(
        modifier
            .background_rounded(theme.secondary, theme.corner_radius)
            .border(1.0, outline, theme.corner_radius),
        || {
            if empty {
                let style = LOCAL_TEXT_STYLE.current().color(theme.on_surface_muted);
                LOCAL_TEXT_STYLE.provide(style, || {
                    text_with(
                        Modifier::new().padding_xy(12.0, 8.0),
                        placeholder.to_string(),
                    );
                });
            }
            with_text_color(theme.on_surface, || {
                basic_text_field_in(
                    Modifier::new(),
                    Modifier::new().padding_xy(12.0, 8.0),
                    state,
                    focus,
                    TextFieldOptions::default(),
                );
            });
        },
    );
}

/// Emits an outlined field of several lines, showing `placeholder` while it is empty.
/// Enter starts a new line. Size it with `modifier`, such as `.width(420.0).height(120.0)`.
#[track_caller]
pub fn text_area(modifier: Modifier, state: &TextFieldState, placeholder: &str) {
    let theme = theme();
    let focus = remember_focus();
    let outline = if focus.is_focused() {
        theme.primary
    } else {
        theme.outline
    };
    let empty = state.text().is_empty();
    stack_with(
        modifier
            .background_rounded(theme.surface, theme.corner_radius)
            .border(1.5, outline, theme.corner_radius)
            .clip(),
        || {
            if empty {
                let style = LOCAL_TEXT_STYLE.current().color(theme.on_surface_muted);
                LOCAL_TEXT_STYLE.provide(style, || {
                    text_with(
                        Modifier::new().padding_xy(12.0, 8.0),
                        placeholder.to_string(),
                    );
                });
            }
            with_text_color(theme.on_surface, || {
                let options = TextFieldOptions {
                    read_only: false,
                    multiline: true,
                };
                basic_text_field_in(
                    Modifier::new().fill_max_size(),
                    Modifier::new().padding_xy(12.0, 8.0),
                    state,
                    &focus,
                    options,
                );
            });
        },
    );
}

/// Emits a bordered box showing `value` with a chevron, which calls `on_click` when clicked.
#[track_caller]
pub fn select_field(modifier: Modifier, value: &str, on_click: impl Fn() + 'static) {
    let theme = theme();
    row_arranged(
        modifier
            .semantics(
                Semantics::new(Role::Button)
                    .label(value.to_string())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .background_rounded(theme.secondary, theme.corner_radius)
            .border(1.0, theme.outline, theme.corner_radius)
            .padding_xy(12.0, 8.0),
        Arrangement::SpaceBetween,
        Alignment::Center,
        || {
            text(value.to_string());
            icon(phosphor::CARET_UP_DOWN, 14.0, theme.on_surface_muted);
        },
    );
}
