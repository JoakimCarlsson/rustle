//! Buttons: text, outlined, icon and floating action buttons, and styled variants.

use rustle_foundation::{
    Alignment, Arrangement, BoxAlignment, LOCAL_TEXT_STYLE, Modifier, Role, Semantics,
    row_arranged, row_with, spacer, stack_aligned, stack_with, text,
};
use rustle_graphics::Color;
use rustle_icons::Icon;
use rustle_text::FontWeight;

use crate::icons::icon;
use crate::theme::theme;

/// Emits a button showing `label` that runs `on_click` when clicked.
#[track_caller]
pub fn button(label: impl Into<String>, on_click: impl Fn() + 'static) {
    button_with(Modifier::new(), label, on_click);
}

/// Emits a button with `modifier`, which applies outside the button's own look.
#[track_caller]
pub fn button_with(modifier: Modifier, label: impl Into<String>, on_click: impl Fn() + 'static) {
    let theme = theme();
    let label = label.into();
    row_with(
        modifier
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.clone())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .background_rounded(theme.primary, theme.corner_radius)
            .padding_xy(16.0, 8.0),
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.on_primary)
                .font_weight(FontWeight::MEDIUM);
            LOCAL_TEXT_STYLE.provide(style, || text(label));
        },
    );
}

/// Emits a button with only a label, no fill.
#[track_caller]
pub fn text_button(label: &str, on_click: impl Fn() + 'static) {
    let theme = theme();
    stack_with(
        Modifier::new()
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.to_string())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .padding_xy(14.0, 8.0),
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.primary)
                .font_weight(FontWeight::MEDIUM);
            LOCAL_TEXT_STYLE.provide(style, || text(label.to_string()));
        },
    );
}

/// Emits a button with an outline and no fill.
#[track_caller]
pub fn outlined_button(label: &str, on_click: impl Fn() + 'static) {
    let theme = theme();
    stack_with(
        Modifier::new()
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.to_string())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .border(1.5, theme.outline, theme.corner_radius)
            .padding_xy(20.0, 10.0),
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.primary)
                .font_weight(FontWeight::MEDIUM);
            LOCAL_TEXT_STYLE.provide(style, || text(label.to_string()));
        },
    );
}

/// Emits a round button showing `glyph`, a short text such as a symbol.
/// `description` is what assistive technology calls it.
#[track_caller]
pub fn icon_button(glyph: &str, description: &str, on_click: impl Fn() + 'static) {
    let theme = theme();
    stack_aligned(
        Modifier::new()
            .size(40.0, 40.0)
            .semantics(Semantics::new(Role::Button).label(description.to_string()))
            .clickable(on_click)
            .background_rounded(Color::TRANSPARENT, 20.0),
        BoxAlignment::Center,
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.on_surface)
                .font_size(20.0);
            rustle_foundation::text_styled(Modifier::new(), glyph.to_string(), style);
        },
    );
}

/// Emits a prominent round button showing `glyph`.
#[track_caller]
pub fn floating_action_button(glyph: &str, description: &str, on_click: impl Fn() + 'static) {
    let theme = theme();
    stack_aligned(
        Modifier::new()
            .size(56.0, 56.0)
            .semantics(Semantics::new(Role::Button).label(description.to_string()))
            .clickable(on_click)
            .shadow(8.0, 28.0)
            .background_rounded(theme.primary, 28.0),
        BoxAlignment::Center,
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.on_primary)
                .font_size(26.0);
            rustle_foundation::text_styled(Modifier::new(), glyph.to_string(), style);
        },
    );
}

/// How a button looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// A solid fill in the foreground color.
    Default,
    /// A subtle fill.
    Secondary,
    /// A border and no fill.
    Outline,
    /// Nothing until it is hovered.
    Ghost,
    /// A warning tint.
    Destructive,
}

/// Emits a button showing `label`, with an optional icon before and after it.
#[track_caller]
pub fn styled_button(
    modifier: Modifier,
    style: ButtonStyle,
    label: &str,
    leading: Option<Icon>,
    trailing: Option<Icon>,
    on_click: impl Fn() + 'static,
) {
    let theme = theme();
    let (fill, content) = match style {
        ButtonStyle::Default => (theme.primary, theme.on_primary),
        ButtonStyle::Secondary => (theme.accent, theme.on_surface),
        ButtonStyle::Outline | ButtonStyle::Ghost => (Color::TRANSPARENT, theme.on_surface),
        ButtonStyle::Destructive => (
            Color::rgba(theme.danger.r, theme.danger.g, theme.danger.b, 36),
            theme.danger,
        ),
    };
    let border = if style == ButtonStyle::Outline {
        theme.outline
    } else {
        Color::TRANSPARENT
    };
    let padding = if leading.is_some() || trailing.is_some() {
        12.0
    } else {
        16.0
    };
    let label_text = label.to_string();
    row_arranged(
        modifier
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.to_string())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .background_rounded(fill, theme.corner_radius)
            .border(1.0, border, theme.corner_radius)
            .padding_xy(padding, 7.0),
        Arrangement::Center,
        Alignment::Center,
        || {
            if let Some(leading) = leading {
                icon(leading, 16.0, content);
                spacer(Modifier::new().width(8.0));
            }
            let text_style = LOCAL_TEXT_STYLE
                .current()
                .color(content)
                .font_weight(FontWeight::MEDIUM);
            LOCAL_TEXT_STYLE.provide(text_style, || text(label_text));
            if let Some(trailing) = trailing {
                spacer(Modifier::new().width(8.0));
                icon(trailing, 16.0, content);
            }
        },
    );
}
