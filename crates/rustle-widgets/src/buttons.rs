//! Buttons: text, outlined, icon and floating action buttons, and styled variants.

use rustle_foundation::{
    Alignment, Arrangement, BoxAlignment, LOCAL_TEXT_STYLE, Modifier, Role, Semantics,
    row_arranged, row_with, spacer, stack_aligned, stack_with, text,
};
use rustle_graphics::Color;
use rustle_icons::Icon;

use crate::icons::icon;
use crate::ripple::RippleModifier;
use crate::shape::{Shape, ShapeModifier};
use crate::theme::{color_scheme, text_style_of, typography};

/// Emits a button showing `label` that runs `on_click` when clicked.
#[track_caller]
pub fn button(label: impl Into<String>, on_click: impl Fn() + 'static) {
    button_with(Modifier::new(), label, on_click);
}

/// Emits a button with `modifier`, which applies outside the button's own look.
#[track_caller]
pub fn button_with(modifier: Modifier, label: impl Into<String>, on_click: impl Fn() + 'static) {
    button_custom(
        modifier,
        label,
        true,
        ButtonDefaults::colors(),
        ButtonDefaults::shape(),
        ButtonDefaults::content_padding(),
        None,
        on_click,
    );
}

/// The colors of a button, enabled and disabled, as Compose's `ButtonColors`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonColors {
    /// Fill behind the label.
    pub container: Color,
    /// Color of the label.
    pub content: Color,
    /// Fill behind the label when the button is disabled.
    pub disabled_container: Color,
    /// Color of the label when the button is disabled.
    pub disabled_content: Color,
}

/// The values a button uses unless told otherwise, read from the current theme, as Compose's
/// `ButtonDefaults`.
///
/// Pass them, or your own, to [`button_custom`]:
/// `ButtonColors { container: brand, ..ButtonDefaults::colors() }`.
pub struct ButtonDefaults;

impl ButtonDefaults {
    /// A filled button: `primary` behind `on_primary`, and `on_surface` washed out when disabled.
    pub fn colors() -> ButtonColors {
        let scheme = color_scheme();
        ButtonColors {
            container: scheme.primary,
            content: scheme.on_primary,
            disabled_container: with_alpha(scheme.on_surface, 0.12),
            disabled_content: with_alpha(scheme.on_surface, 0.38),
        }
    }

    /// A tonal button: `secondary_container` behind `on_secondary_container`.
    pub fn secondary_colors() -> ButtonColors {
        let scheme = color_scheme();
        ButtonColors {
            container: scheme.secondary_container,
            content: scheme.on_secondary_container,
            ..Self::colors()
        }
    }

    /// An outlined button: no fill and `primary` text.
    pub fn outlined_colors() -> ButtonColors {
        ButtonColors {
            container: Color::TRANSPARENT,
            content: color_scheme().primary,
            disabled_container: Color::TRANSPARENT,
            ..Self::colors()
        }
    }

    /// A text button: no fill and `primary` text.
    pub fn text_colors() -> ButtonColors {
        Self::outlined_colors()
    }

    /// The border of an outlined button: width and `outline` color.
    pub fn outlined_border() -> (f32, Color) {
        (1.0, color_scheme().outline)
    }

    /// The corner radius of buttons: fully round, as in Compose.
    pub fn shape() -> Shape {
        Shape::Circle
    }

    /// The horizontal and vertical padding between a button's edge and its label.
    pub fn content_padding() -> (f32, f32) {
        (16.0, 8.0)
    }
}

/// `color` at `alpha` of its own opacity, from `0.0` to `1.0`.
fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::rgba(
        color.r,
        color.g,
        color.b,
        (f32::from(color.a) * alpha) as u8,
    )
}

/// Emits a button with every part of its look given, as Compose's `Button` does with its
/// parameters: `colors`, the corner radius `shape`, the horizontal and vertical `padding`, and
/// an optional `border` of width and color. A button that is not `enabled` takes the disabled
/// colors and ignores clicks.
#[track_caller]
#[allow(clippy::too_many_arguments)]
pub fn button_custom(
    modifier: Modifier,
    label: impl Into<String>,
    enabled: bool,
    colors: ButtonColors,
    shape: Shape,
    padding: (f32, f32),
    border: Option<(f32, Color)>,
    on_click: impl Fn() + 'static,
) {
    let label = label.into();
    let (container, content) = if enabled {
        (colors.container, colors.content)
    } else {
        (colors.disabled_container, colors.disabled_content)
    };
    let mut modifier = modifier.semantics(
        Semantics::new(Role::Button)
            .label(label.clone())
            .merging_descendants(),
    );
    if enabled {
        modifier = modifier.ripple_clickable(shape, content, on_click);
    }
    modifier = modifier.background_shape(container, shape);
    if let Some((width, color)) = border {
        modifier = modifier.border_shape(width, color, shape);
    }
    row_with(modifier.padding_xy(padding.0, padding.1), || {
        let style = typography().label_large.color(content);
        LOCAL_TEXT_STYLE.provide(style, || text(label));
    });
}

/// Emits a button with only a label, no fill.
#[track_caller]
pub fn text_button(label: &str, on_click: impl Fn() + 'static) {
    let theme = color_scheme();
    stack_with(
        Modifier::new()
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.to_string())
                    .merging_descendants(),
            )
            .ripple_clickable(Shape::Circle, theme.primary, on_click)
            .padding_xy(14.0, 8.0),
        || {
            let style = text_style_of(typography().label_large).color(theme.primary);
            LOCAL_TEXT_STYLE.provide(style, || text(label.to_string()));
        },
    );
}

/// Emits a button with an outline and no fill.
#[track_caller]
pub fn outlined_button(label: &str, on_click: impl Fn() + 'static) {
    let theme = color_scheme();
    stack_with(
        Modifier::new()
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.to_string())
                    .merging_descendants(),
            )
            .ripple_clickable(Shape::Circle, theme.primary, on_click)
            .border_shape(1.5, theme.outline, Shape::Circle)
            .padding_xy(20.0, 10.0),
        || {
            let style = text_style_of(typography().label_large).color(theme.primary);
            LOCAL_TEXT_STYLE.provide(style, || text(label.to_string()));
        },
    );
}

/// Emits a round button showing `glyph`, a short text such as a symbol.
/// `description` is what assistive technology calls it.
#[track_caller]
pub fn icon_button(glyph: &str, description: &str, on_click: impl Fn() + 'static) {
    let theme = color_scheme();
    stack_aligned(
        Modifier::new()
            .size(40.0, 40.0)
            .semantics(Semantics::new(Role::Button).label(description.to_string()))
            .ripple_clickable(Shape::Circle, theme.on_surface, on_click)
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
    let theme = color_scheme();
    stack_aligned(
        Modifier::new()
            .size(56.0, 56.0)
            .semantics(Semantics::new(Role::Button).label(description.to_string()))
            .ripple_clickable(Shape::rounded(28.0), theme.on_primary, on_click)
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
    let theme = color_scheme();
    let (fill, content) = match style {
        ButtonStyle::Default => (theme.primary, theme.on_primary),
        ButtonStyle::Secondary => (theme.surface_variant, theme.on_surface),
        ButtonStyle::Outline | ButtonStyle::Ghost => (Color::TRANSPARENT, theme.on_surface),
        ButtonStyle::Destructive => (
            Color::rgba(theme.error.r, theme.error.g, theme.error.b, 36),
            theme.error,
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
            .ripple_clickable(Shape::Circle, content, on_click)
            .background_shape(fill, Shape::Circle)
            .border_shape(1.0, border, Shape::Circle)
            .padding_xy(padding, 7.0),
        Arrangement::Center,
        Alignment::Center,
        || {
            if let Some(leading) = leading {
                icon(leading, 16.0, content);
                spacer(Modifier::new().width(8.0));
            }
            let text_style = text_style_of(typography().label_large).color(content);
            LOCAL_TEXT_STYLE.provide(text_style, || text(label_text));
            if let Some(trailing) = trailing {
                spacer(Modifier::new().width(8.0));
                icon(trailing, 16.0, content);
            }
        },
    );
}
