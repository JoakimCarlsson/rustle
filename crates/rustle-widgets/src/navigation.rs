//! Tabs, segmented controls and buttons, and the bottom navigation bar.

use std::rc::Rc;

use rustle_foundation::{
    Alignment, Arrangement, LOCAL_TEXT_STYLE, Modifier, Role, Semantics, column_arranged,
    remember_scroll_state, row_arranged, row_with, stack_with, text_styled, text_with,
};
use rustle_graphics::Color;
use rustle_runtime::key;
use rustle_text::FontWeight;

use crate::BadgeStyle;
use crate::icons::icon;
use crate::pill;
use crate::shape::{BorderStroke, Shape, ShapeModifier};
use crate::theme::{color_scheme, provide_content_color, shapes, text_style_of, typography};

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

/// The colors of a segmented button in each of its states: Compose's `SegmentedButtonColors`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentedButtonColors {
    /// The fill of the selected button.
    pub active_container_color: Color,
    /// The text and icon of the selected button.
    pub active_content_color: Color,
    /// The border of the selected button.
    pub active_border_color: Color,
    /// The fill of a button not selected.
    pub inactive_container_color: Color,
    /// The text and icon of a button not selected.
    pub inactive_content_color: Color,
    /// The border of a button not selected.
    pub inactive_border_color: Color,
    /// The fill of the selected button while it is disabled.
    pub disabled_active_container_color: Color,
    /// The text and icon of the selected button while it is disabled.
    pub disabled_active_content_color: Color,
    /// The border of the selected button while it is disabled.
    pub disabled_active_border_color: Color,
    /// The fill of a button not selected while it is disabled.
    pub disabled_inactive_container_color: Color,
    /// The text and icon of a button not selected while it is disabled.
    pub disabled_inactive_content_color: Color,
    /// The border of a button not selected while it is disabled.
    pub disabled_inactive_border_color: Color,
}

impl SegmentedButtonColors {
    /// The fill of a button that is `enabled` or not and `active` (selected) or not:
    /// Compose's `containerColor(enabled, active)`.
    pub fn container_color(&self, enabled: bool, active: bool) -> Color {
        match (enabled, active) {
            (true, true) => self.active_container_color,
            (true, false) => self.inactive_container_color,
            (false, true) => self.disabled_active_container_color,
            (false, false) => self.disabled_inactive_container_color,
        }
    }

    /// The text and icon color of such a button: Compose's `contentColor(enabled, checked)`.
    pub fn content_color(&self, enabled: bool, active: bool) -> Color {
        match (enabled, active) {
            (true, true) => self.active_content_color,
            (true, false) => self.inactive_content_color,
            (false, true) => self.disabled_active_content_color,
            (false, false) => self.disabled_inactive_content_color,
        }
    }

    /// The border color of such a button: Compose's `borderColor(enabled, active)`.
    pub fn border_color(&self, enabled: bool, active: bool) -> Color {
        match (enabled, active) {
            (true, true) => self.active_border_color,
            (true, false) => self.inactive_border_color,
            (false, true) => self.disabled_active_border_color,
            (false, false) => self.disabled_inactive_border_color,
        }
    }
}

/// The default look of segmented buttons: Compose's `SegmentedButtonDefaults`.
pub struct SegmentedButtonDefaults;

impl SegmentedButtonDefaults {
    /// How thick the border of a segmented button is, and how far neighbours overlap so they
    /// share one border: `SegmentedButtonDefaults.BorderWidth`.
    pub const BORDER_WIDTH: f32 = 1.0;

    /// How large the icon of a segmented button is: `SegmentedButtonDefaults.IconSize`.
    pub const ICON_SIZE: f32 = 18.0;

    /// The room around a segmented button's icon and label, horizontal and vertical:
    /// `SegmentedButtonDefaults.ContentPadding`.
    pub const CONTENT_PADDING: (f32, f32) = (12.0, 8.0);

    /// The colors from the theme: the secondary container behind the selected button, the
    /// outline around every button, as `SegmentedButtonDefaults.colors()`. Change fields with
    /// struct update syntax for Compose's `colors(activeContainerColor = …)`.
    pub fn colors() -> SegmentedButtonColors {
        let scheme = color_scheme();
        let disabled = with_alpha(scheme.on_surface, 0.38);
        SegmentedButtonColors {
            active_container_color: scheme.secondary_container,
            active_content_color: scheme.on_secondary_container,
            active_border_color: scheme.outline,
            inactive_container_color: scheme.surface,
            inactive_content_color: scheme.on_surface,
            inactive_border_color: scheme.outline,
            disabled_active_container_color: scheme.secondary_container,
            disabled_active_content_color: disabled,
            disabled_active_border_color: with_alpha(scheme.on_surface, 0.12),
            disabled_inactive_container_color: scheme.surface,
            disabled_inactive_content_color: disabled,
            disabled_inactive_border_color: scheme.outline,
        }
    }

    /// The shape the row of buttons is cut from: fully rounded ends, as
    /// `SegmentedButtonDefaults.baseShape`.
    pub fn base_shape() -> Shape {
        Shape::rounded(Shape::FULL)
    }

    /// The shape of the button at `index` of `count`: the outer corners of `base_shape` on
    /// the first and last, square corners between, as `SegmentedButtonDefaults.itemShape`.
    pub fn item_shape(index: usize, count: usize, base_shape: Shape) -> Shape {
        if count <= 1 {
            return base_shape;
        }
        let corners = match base_shape {
            Shape::Rectangle => return Shape::Rectangle,
            Shape::Circle => crate::shape::CornerSizes::uniform(Shape::FULL),
            Shape::Rounded(corners) | Shape::Cut(corners) => corners,
        };
        let first = index == 0;
        let last = index + 1 == count;
        let sizes = crate::shape::CornerSizes::new(
            if first { corners.top_left } else { 0.0 },
            if last { corners.top_right } else { 0.0 },
            if last { corners.bottom_right } else { 0.0 },
            if first { corners.bottom_left } else { 0.0 },
        );
        match base_shape {
            Shape::Cut(_) => Shape::Cut(sizes),
            _ => Shape::Rounded(sizes),
        }
    }

    /// The border of a segmented button: `width` thick in `color`, as
    /// `SegmentedButtonDefaults.borderStroke(color, width)`.
    pub fn border_stroke(color: Color, width: f32) -> BorderStroke {
        BorderStroke::new(width, color)
    }

    /// Emits the icon a segmented button shows by default: a check while `active`, nothing
    /// otherwise, as `SegmentedButtonDefaults.Icon(active)`.
    #[track_caller]
    pub fn icon(active: bool) {
        if active {
            icon(
                rustle_icons::phosphor::CHECK,
                Self::ICON_SIZE,
                crate::theme::content_color(),
            );
        }
    }
}

/// `color` with its opacity multiplied by `alpha`, from 0.0 to 1.0.
fn with_alpha(color: Color, alpha: f32) -> Color {
    Color {
        a: (f32::from(color.a) * alpha).round() as u8,
        ..color
    }
}

/// Emits a row of segmented buttons of which one is picked at a time, emitted by `content`
/// with [`segmented_button`], each overlapping the one before it by `space` so neighbours
/// share a border: Compose's `SingleChoiceSegmentedButtonRow(modifier, space, content)`.
/// Give each button its place with [`SegmentedButtonDefaults::item_shape`].
#[track_caller]
pub fn single_choice_segmented_button_row(modifier: Modifier, space: f32, content: impl FnOnce()) {
    row_arranged(
        modifier
            .height_intrinsic(rustle_foundation::IntrinsicSize::Min)
            .semantics(Semantics::new(Role::Group)),
        Arrangement::SpacedBy(-space),
        Alignment::Center,
        content,
    );
}

/// Emits a button of a [`single_choice_segmented_button_row`]: filled, outlined and colored
/// by `colors` as it is `selected` and `enabled`, in `shape` with `border` along it,
/// `icon` then `label` inside `content_padding` (horizontal, vertical), and calling
/// `on_click` when clicked while `enabled`, with the hover and press feedback of
/// [`Modifier::clickable`]. Compose's `SegmentedButton(selected, onClick,
/// shape, modifier, enabled, colors, border, contentPadding, icon, label)`; pass
/// [`SegmentedButtonDefaults::icon`] for Compose's default icon. The selected button is drawn
/// over its neighbours, so its border shows where they overlap. Its size is what `modifier`,
/// the padding and the content make it.
#[track_caller]
#[allow(clippy::too_many_arguments)]
pub fn segmented_button(
    selected: bool,
    on_click: impl Fn() + 'static,
    shape: Shape,
    modifier: Modifier,
    enabled: bool,
    colors: SegmentedButtonColors,
    border: BorderStroke,
    content_padding: (f32, f32),
    icon: impl FnOnce(),
    label: impl FnOnce(),
) {
    let container = colors.container_color(enabled, selected);
    let content = colors.content_color(enabled, selected);
    let mut modifier = modifier
        .z_index(i32::from(selected))
        .fill_max_height()
        .semantics(
            Semantics::new(Role::RadioButton)
                .toggled(selected)
                .merging_descendants(),
        );
    if enabled {
        modifier = modifier.clickable(on_click);
    }
    let modifier = modifier
        .background_shape(container, shape)
        .border_shape(border.width, border.color, shape)
        .padding_xy(content_padding.0, content_padding.1);
    row_arranged(modifier, Arrangement::Center, Alignment::Center, || {
        provide_content_color(content, || {
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(8.0),
                Alignment::Center,
                || {
                    icon();
                    label();
                },
            );
        });
    });
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

#[cfg(test)]
mod tests;
