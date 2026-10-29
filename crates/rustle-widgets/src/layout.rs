//! Surfaces, app bars, dividers and text helpers that structure a screen.

use rustle_foundation::{
    Alignment, Arrangement, LOCAL_TEXT_STYLE, Modifier, column, column_with, row_arranged, spacer,
    stack_with, text, text_styled,
};
use rustle_text::FontWeight;

use crate::theme::{theme, with_text_color};

/// Emits a container filled with the theme's surface color, around `content`.
#[track_caller]
pub fn surface(content: impl FnOnce()) {
    surface_with(Modifier::new(), content);
}

/// Emits a surface with `modifier`, which applies outside the surface's own look.
#[track_caller]
pub fn surface_with(modifier: Modifier, content: impl FnOnce()) {
    let theme = theme();
    let radius = theme.corner_radius + 4.0;
    stack_with(
        modifier
            .background_rounded(theme.surface, radius)
            .border(1.0, theme.outline, radius),
        || with_text_color(theme.on_surface, content),
    );
}

/// Emits a surface: a card with a hairline border.
#[track_caller]
pub fn card(modifier: Modifier, content: impl FnOnce()) {
    surface_with(modifier, content);
}

/// Emits a card heading: a title with a muted line of description under it.
#[track_caller]
pub fn card_header(heading: &str, description: &str) {
    column(|| {
        title(heading, 14.0);
        if !description.is_empty() {
            muted_sized(description, 13.0);
        }
    });
}

/// Emits a thin line across the available width.
#[track_caller]
pub fn divider() {
    let theme = theme();
    spacer(
        Modifier::new()
            .fill_max_width()
            .height(1.0)
            .background(theme.outline),
    );
}

/// Emits a hairline across the available width.
#[track_caller]
pub fn separator() {
    let theme = theme();
    spacer(
        Modifier::new()
            .fill_max_width()
            .height(1.0)
            .background(theme.outline),
    );
}

/// Emits a bar across the top showing `title`, in the theme's primary color.
#[track_caller]
pub fn top_app_bar(title: &str) {
    let theme = theme();
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .height(56.0)
            .background(theme.primary)
            .padding_xy(20.0, 0.0),
        Arrangement::Start,
        Alignment::Center,
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.on_primary)
                .font_size(21.0)
                .bold();
            text_styled(Modifier::new(), title.to_string(), style);
        },
    );
}

/// Emits `top_bar` above `content`, with `content` taking the rest of the space.
#[track_caller]
pub fn scaffold(top_bar: impl FnOnce(), content: impl FnOnce()) {
    column_with(Modifier::new().fill_max_size(), || {
        top_bar();
        stack_with(Modifier::new().fill_max_width().weight(1.0), content);
    });
}

/// Emits text in the muted color, for descriptions and captions.
#[track_caller]
pub fn muted(content: &str) {
    muted_sized(content, 14.0);
}

/// Emits muted text of a given size.
#[track_caller]
pub fn muted_sized(content: &str, size: f32) {
    let theme = theme();
    let text_style = LOCAL_TEXT_STYLE
        .current()
        .color(theme.on_surface_muted)
        .font_size(size);
    text_styled(Modifier::new(), content.to_string(), text_style);
}

/// Emits text in a semibold weight, for titles.
#[track_caller]
pub fn title(content: &str, size: f32) {
    let text_style = LOCAL_TEXT_STYLE
        .current()
        .font_size(size)
        .font_weight(FontWeight::SEMI_BOLD);
    text_styled(Modifier::new(), content.to_string(), text_style);
}

/// Emits a row with a `headline`, an optional line of `supporting` text under
/// it and optional `trailing` text at the end.
#[track_caller]
pub fn list_item(headline: &str, supporting: Option<&str>, trailing: Option<&str>) {
    let theme = theme();
    row_arranged(
        Modifier::new().fill_max_width().padding_xy(16.0, 10.0),
        Arrangement::SpaceBetween,
        Alignment::Center,
        || {
            column(|| {
                text(headline.to_string());
                if let Some(supporting) = supporting {
                    let style = LOCAL_TEXT_STYLE
                        .current()
                        .color(theme.on_surface_muted)
                        .font_size(14.0);
                    rustle_foundation::text_styled(Modifier::new(), supporting.to_string(), style);
                }
            });
            if let Some(trailing) = trailing {
                let style = LOCAL_TEXT_STYLE.current().color(theme.on_surface_muted);
                rustle_foundation::text_styled(Modifier::new(), trailing.to_string(), style);
            }
        },
    );
}
