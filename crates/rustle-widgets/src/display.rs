//! Small display widgets: chips, badges, pills and avatars.

use rustle_animation::{animate_color_as_state, tween};
use rustle_foundation::{
    BoxAlignment, LOCAL_TEXT_STYLE, Modifier, Role, Semantics, stack_aligned, stack_with, text,
    text_styled,
};
use rustle_graphics::Color;
use rustle_text::FontWeight;

use crate::theme::{color_scheme, text_style_of, typography};

/// Emits a small rounded label that can be selected, calling `on_click` when clicked.
#[track_caller]
pub fn chip(label: &str, selected: bool, on_click: impl Fn() + 'static) {
    let theme = color_scheme();
    let fill = animate_color_as_state(
        if selected {
            theme.primary
        } else {
            Color::TRANSPARENT
        },
        tween(150),
    );
    stack_with(
        Modifier::new()
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.to_string())
                    .toggled(selected)
                    .merging_descendants(),
            )
            .clickable(on_click)
            .background_rounded(fill.get(), 8.0)
            .border(
                1.0,
                if selected {
                    theme.primary
                } else {
                    theme.outline
                },
                8.0,
            )
            .padding_xy(14.0, 6.0),
        || {
            let color = if selected {
                theme.on_primary
            } else {
                theme.on_surface
            };
            let style = text_style_of(typography().label_large).color(color);
            LOCAL_TEXT_STYLE.provide(style, || text(label.to_string()));
        },
    );
}

/// Emits a small red circle showing `count`, to put beside something it counts.
#[track_caller]
pub fn badge(count: u32) {
    stack_aligned(
        Modifier::new()
            .width_in(18.0, 40.0)
            .height(18.0)
            .semantics(Semantics::new(Role::Label).label(format!("{count} notifications")))
            .background_rounded(Color::rgb(214, 64, 64), 9.0)
            .padding_xy(5.0, 0.0),
        BoxAlignment::Center,
        || {
            let style = text_style_of(typography().label_medium).color(Color::rgb(255, 255, 255));
            rustle_foundation::text_styled(Modifier::new(), count.to_string(), style);
        },
    );
}

/// How a badge looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeStyle {
    Default,
    Secondary,
    Outline,
}

/// Emits a small rounded label such as a status or a count.
#[track_caller]
pub fn pill(label: &str, style: BadgeStyle) {
    let theme = color_scheme();
    let (fill, content, border) = match style {
        BadgeStyle::Default => (theme.primary, theme.on_primary, Color::TRANSPARENT),
        BadgeStyle::Secondary => (theme.surface_variant, theme.on_surface, Color::TRANSPARENT),
        BadgeStyle::Outline => (Color::TRANSPARENT, theme.on_surface, theme.outline),
    };
    stack_with(
        Modifier::new()
            .background_rounded(fill, 100.0)
            .border(1.0, border, 100.0)
            .padding_xy(8.0, 1.0),
        || {
            let text_style = text_style_of(typography().label_medium).color(content);
            text_styled(Modifier::new(), label.to_string(), text_style);
        },
    );
}

/// Emits a round avatar of `size` showing `initials`.
#[track_caller]
pub fn avatar(initials: &str, size: f32) {
    let theme = color_scheme();
    stack_aligned(
        Modifier::new()
            .size(size, size)
            .background_rounded(theme.surface_variant, size / 2.0),
        BoxAlignment::Center,
        || {
            let text_style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.on_surface)
                .font_size((size * 0.38).max(10.0))
                .font_weight(FontWeight::MEDIUM);
            text_styled(Modifier::new(), initials.to_string(), text_style);
        },
    );
}
