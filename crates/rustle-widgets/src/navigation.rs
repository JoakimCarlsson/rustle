//! Tabs, segmented controls and the bottom navigation bar.

use std::rc::Rc;

use rustle_foundation::{
    Alignment, Arrangement, LOCAL_TEXT_STYLE, Modifier, Role, Semantics, column_arranged,
    remember_scroll_state, row_arranged, row_with, stack_with, text_styled, text_with,
};
use rustle_graphics::Color;
use rustle_runtime::key;
use rustle_text::FontWeight;

use crate::BadgeStyle;
use crate::pill;
use crate::shape::ShapeModifier;
use crate::theme::{color_scheme, shapes, text_style_of, typography};

/// Emits a row of tabs labelled `labels`, with `selected` underlined, calling
/// `on_select` with the index of the tab clicked.
#[track_caller]
pub fn tab_row(selected: usize, labels: &[&str], on_select: impl Fn(usize) + 'static) {
    let theme = color_scheme();
    let on_select = Rc::new(on_select);
    // Tabs keep their natural width and the row scrolls when they do not all fit.
    let scroll = remember_scroll_state();
    row_with(
        Modifier::new().fill_max_width().horizontal_scroll(&scroll),
        || {
            for (index, label) in labels.iter().enumerate() {
                let on_select = Rc::clone(&on_select);
                key(index, || {
                    let active = index == selected;
                    let color = if active {
                        theme.primary
                    } else {
                        theme.on_surface_variant
                    };
                    let style = LOCAL_TEXT_STYLE
                        .current()
                        .color(color)
                        .font_weight(if active {
                            FontWeight::SEMI_BOLD
                        } else {
                            FontWeight::NORMAL
                        });
                    LOCAL_TEXT_STYLE.provide(style, || {
                        text_with(
                            Modifier::new()
                                .semantics(
                                    Semantics::new(Role::Tab)
                                        .label(label.to_string())
                                        .toggled(active)
                                        .merging_descendants(),
                                )
                                .clickable(move || on_select(index))
                                .border_bottom(
                                    2.0,
                                    if active {
                                        theme.primary
                                    } else {
                                        Color::TRANSPARENT
                                    },
                                )
                                .padding_xy(18.0, 8.0),
                            label.to_string(),
                        );
                    });
                });
            }
        },
    );
}

/// Emits tabs in a rounded tray, one per `(label, count)`, with `selected` raised.
#[track_caller]
pub fn tabs_list(
    selected: usize,
    tabs: &[(&str, Option<u32>)],
    on_select: impl Fn(usize) + 'static,
) {
    let theme = color_scheme();
    let on_select = Rc::new(on_select);
    row_arranged(
        Modifier::new()
            .background_shape(theme.secondary_container, shapes().small)
            .border_shape(1.0, theme.outline, shapes().small)
            .padding(3.0),
        Arrangement::SpacedBy(2.0),
        Alignment::Center,
        || {
            for (index, (label, count)) in tabs.iter().enumerate() {
                let on_select = Rc::clone(&on_select);
                key(index, || {
                    let active = index == selected;
                    row_arranged(
                        Modifier::new()
                            .semantics(
                                Semantics::new(Role::Tab)
                                    .label(label.to_string())
                                    .toggled(active)
                                    .merging_descendants(),
                            )
                            .clickable(move || on_select(index))
                            .background_shape(
                                if active {
                                    theme.surface_variant
                                } else {
                                    Color::TRANSPARENT
                                },
                                shapes().small,
                            )
                            .padding_xy(12.0, 5.0),
                        Arrangement::SpacedBy(6.0),
                        Alignment::Center,
                        || {
                            let text_style =
                                text_style_of(typography().label_large).color(if active {
                                    theme.on_surface
                                } else {
                                    theme.on_surface_variant
                                });
                            text_styled(Modifier::new(), label.to_string(), text_style);
                            if let Some(count) = count {
                                pill(&count.to_string(), BadgeStyle::Secondary);
                            }
                        },
                    );
                });
            }
        },
    );
}

/// Emits joined buttons, one per label, with `selected` highlighted.
#[track_caller]
pub fn segmented_control(selected: usize, labels: &[&str], on_select: impl Fn(usize) + 'static) {
    let theme = color_scheme();
    let on_select = Rc::new(on_select);
    row_arranged(
        Modifier::new()
            .border_shape(1.0, theme.outline, shapes().small)
            .clip(),
        Arrangement::Start,
        Alignment::Center,
        || {
            for (index, label) in labels.iter().enumerate() {
                let on_select = Rc::clone(&on_select);
                key(index, || {
                    let active = index == selected;
                    stack_with(
                        Modifier::new()
                            .semantics(
                                Semantics::new(Role::Tab)
                                    .label(label.to_string())
                                    .toggled(active)
                                    .merging_descendants(),
                            )
                            .clickable(move || on_select(index))
                            .background(if active {
                                theme.surface_variant
                            } else {
                                Color::TRANSPARENT
                            })
                            .padding_xy(14.0, 6.0),
                        || {
                            let text_style =
                                text_style_of(typography().label_large).color(if active {
                                    theme.on_surface
                                } else {
                                    theme.on_surface_variant
                                });
                            text_styled(Modifier::new(), label.to_string(), text_style);
                        },
                    );
                });
            }
        },
    );
}

/// Emits a bar of destinations, one per `(glyph, label)`, with `selected`
/// highlighted, calling `on_select` with the index of the one clicked.
#[track_caller]
pub fn navigation_bar(
    selected: usize,
    items: &[(&str, &str)],
    on_select: impl Fn(usize) + 'static,
) {
    let theme = color_scheme();
    let on_select = Rc::new(on_select);
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .background(theme.surface)
            .padding_xy(8.0, 8.0),
        Arrangement::SpaceEvenly,
        Alignment::Center,
        || {
            for (index, (glyph, label)) in items.iter().enumerate() {
                let on_select = Rc::clone(&on_select);
                key(index, || {
                    let active = index == selected;
                    let color = if active {
                        theme.primary
                    } else {
                        theme.on_surface_variant
                    };
                    column_arranged(
                        Modifier::new()
                            .semantics(
                                Semantics::new(Role::Tab)
                                    .label(label.to_string())
                                    .toggled(active)
                                    .merging_descendants(),
                            )
                            .clickable(move || on_select(index))
                            .padding_xy(16.0, 4.0),
                        Arrangement::Start,
                        Alignment::Center,
                        || {
                            let icon = LOCAL_TEXT_STYLE.current().color(color).font_size(22.0);
                            rustle_foundation::text_styled(
                                Modifier::new(),
                                glyph.to_string(),
                                icon,
                            );
                            let caption = text_style_of(typography().label_medium).color(color);
                            rustle_foundation::text_styled(
                                Modifier::new(),
                                label.to_string(),
                                caption,
                            );
                        },
                    );
                });
            }
        },
    );
}
