//! Toggles and sliders.

use rustle_animation::{animate_color_as_state, animate_float_as_state, tween};
use rustle_foundation::{BoxAlignment, Modifier, Role, Semantics, canvas, spacer, stack_aligned};
use rustle_graphics::Color;
use rustle_icons::phosphor;
use rustle_runtime::remember;

use crate::icons::icon;
use crate::theme::color_scheme;

/// Reads a value assistive technology sets on a toggle: `true`, `on`, `checked` or `1`
/// turn it on, `false`, `off`, `unchecked` or `0` turn it off, anything else is ignored.
fn parse_toggle(text: &str) -> Option<bool> {
    match text.trim().to_lowercase().as_str() {
        "true" | "on" | "checked" | "1" => Some(true),
        "false" | "off" | "unchecked" | "0" => Some(false),
        _ => None,
    }
}

/// Reads a percentage such as `40` or `40%` as a fraction from 0 to 1.
fn parse_percent(text: &str) -> Option<f32> {
    let number: f32 = text.trim().trim_end_matches('%').trim().parse().ok()?;
    number.is_finite().then(|| (number / 100.0).clamp(0.0, 1.0))
}

/// Emits a box that is checked or not, and calls `on_change` with the opposite
/// value when clicked. The caller owns the state.
#[track_caller]
pub fn checkbox(checked: bool, on_change: impl Fn(bool) + 'static) {
    let on_change = std::rc::Rc::new(on_change);
    let set = std::rc::Rc::clone(&on_change);
    let theme = color_scheme();
    let fill = animate_color_as_state(
        if checked {
            theme.primary
        } else {
            Color::TRANSPARENT
        },
        tween(120),
    );
    let outline = if checked {
        theme.primary
    } else {
        theme.outline
    };
    stack_aligned(
        Modifier::new()
            .size(16.0, 16.0)
            .semantics(
                Semantics::new(Role::CheckBox)
                    .toggled(checked)
                    .on_set_value(move |text| {
                        if let Some(on) = parse_toggle(&text) {
                            set(on);
                        }
                    }),
            )
            .clickable(move || on_change(!checked))
            .background_rounded(fill.get(), 4.0)
            .border(1.0, outline, 4.0),
        BoxAlignment::Center,
        || {
            if checked {
                icon(phosphor::bold::CHECK, 12.0, theme.on_primary);
            }
        },
    );
}

/// The state of a check box that also stands for others, as Compose's `ToggleableState`:
/// ticked, clear, or ticked for only some of what it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleableState {
    /// Ticked.
    On,
    /// Clear.
    Off,
    /// Ticked for some of what it stands for: drawn with a dash.
    Indeterminate,
}

impl From<bool> for ToggleableState {
    /// `On` for true, `Off` for false.
    fn from(on: bool) -> Self {
        if on { Self::On } else { Self::Off }
    }
}

/// The colors of a check box, as Compose's `CheckboxColors`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckboxColors {
    /// The mark while ticked or dashed.
    pub checked_checkmark_color: Color,
    /// The fill of the box while ticked or dashed.
    pub checked_box_color: Color,
    /// The fill of the box while clear.
    pub unchecked_box_color: Color,
    /// The outline while ticked or dashed.
    pub checked_border_color: Color,
    /// The outline while clear.
    pub unchecked_border_color: Color,
    /// The fill of a switched off box while ticked or dashed.
    pub disabled_checked_box_color: Color,
    /// The outline of a switched off box.
    pub disabled_border_color: Color,
}

/// The default look of check boxes, as Compose's `CheckboxDefaults`.
pub struct CheckboxDefaults;

impl CheckboxDefaults {
    /// The side of a check box given no size of its own, in logical pixels.
    pub const SIZE: f32 = 16.0;

    /// The colors of a check box in the theme: filled in its primary color with the mark in
    /// the color on it while ticked, outlined in its outline color while clear, as
    /// `CheckboxDefaults.colors()`.
    pub fn colors() -> CheckboxColors {
        let theme = color_scheme();
        CheckboxColors {
            checked_checkmark_color: theme.on_primary,
            checked_box_color: theme.primary,
            unchecked_box_color: Color::TRANSPARENT,
            checked_border_color: theme.primary,
            unchecked_border_color: theme.outline,
            disabled_checked_box_color: with_alpha(theme.on_surface, 0.38),
            disabled_border_color: with_alpha(theme.on_surface, 0.38),
        }
    }
}

/// `color` at `alpha` of its own opacity.
fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::rgba(
        color.r,
        color.g,
        color.b,
        (f32::from(color.a) * alpha).round() as u8,
    )
}

/// Emits a check box with every part of its look given, as Compose's
/// `Checkbox(checked, onCheckedChange, modifier, enabled, colors)`: see [`tri_state_checkbox`].
/// `on_checked_change` is called with the opposite of `checked` when it is clicked; with
/// `None` the box does not answer the pointer itself, for a box inside a larger control that
/// does.
#[track_caller]
pub fn checkbox_with(
    modifier: Modifier,
    checked: bool,
    on_checked_change: Option<Box<dyn Fn(bool)>>,
    enabled: bool,
    colors: CheckboxColors,
) {
    let on_click =
        on_checked_change.map(|change| -> Box<dyn Fn()> { Box::new(move || change(!checked)) });
    tri_state_checkbox(
        modifier,
        ToggleableState::from(checked),
        on_click,
        enabled,
        colors,
    );
}

/// Emits a check box that is ticked, clear or dashed, as Compose's
/// `TriStateCheckbox(state, onClick, modifier, enabled, colors)`. The box fills the size its
/// `modifier` gives it, [`CheckboxDefaults::SIZE`] square otherwise, with corners rounded by a
/// quarter of its side, and the mark takes three quarters of it. `on_click` runs when it is
/// clicked, unless it is `None` or the box is not `enabled`.
#[track_caller]
pub fn tri_state_checkbox(
    modifier: Modifier,
    state: ToggleableState,
    on_click: Option<Box<dyn Fn()>>,
    enabled: bool,
    colors: CheckboxColors,
) {
    let marked = state != ToggleableState::Off;
    let (fill, outline) = match (marked, enabled) {
        (true, true) => (colors.checked_box_color, colors.checked_border_color),
        (true, false) => (
            colors.disabled_checked_box_color,
            colors.disabled_border_color,
        ),
        (false, true) => (colors.unchecked_box_color, colors.unchecked_border_color),
        (false, false) => (colors.unchecked_box_color, colors.disabled_border_color),
    };
    let mark = match state {
        ToggleableState::On => Some(phosphor::bold::CHECK),
        ToggleableState::Indeterminate => Some(phosphor::bold::MINUS),
        ToggleableState::Off => None,
    };
    let ink = colors.checked_checkmark_color;
    let mut modifier = modifier
        .size(CheckboxDefaults::SIZE, CheckboxDefaults::SIZE)
        .semantics(Semantics::new(Role::CheckBox).toggled(marked));
    if let (Some(on_click), true) = (on_click, enabled) {
        modifier = modifier.clickable(on_click);
    }
    canvas(modifier, move |canvas| {
        let side = canvas.width().min(canvas.height());
        let corner = side / 4.0;
        let (width, height) = (canvas.width(), canvas.height());
        canvas.rounded_rect(0.0, 0.0, width, height, corner, fill);
        canvas.stroke_rect(0.0, 0.0, width, height, corner, 1.0, outline);
        if let Some(mark) = &mark {
            let size = side * 0.75;
            canvas.path(
                mark,
                (width - size) / 2.0,
                (height - size) / 2.0,
                size,
                size,
                ink,
            );
        }
    });
}

/// The colors of a switch, as Compose's `SwitchColors`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchColors {
    /// The knob while on.
    pub checked_thumb_color: Color,
    /// The track while on.
    pub checked_track_color: Color,
    /// The knob while off.
    pub unchecked_thumb_color: Color,
    /// The track while off.
    pub unchecked_track_color: Color,
}

/// The default look of switches, as Compose's `SwitchDefaults`.
pub struct SwitchDefaults;

impl SwitchDefaults {
    /// The width of a switch given no size of its own, in logical pixels.
    pub const WIDTH: f32 = 36.0;
    /// The height of a switch given no size of its own, in logical pixels.
    pub const HEIGHT: f32 = 20.0;

    /// The colors of a switch in the theme: a primary track with the color on it as the knob
    /// while on, a surface variant track with the color on the surface as the knob while off,
    /// as `SwitchDefaults.colors()`.
    pub fn colors() -> SwitchColors {
        let theme = color_scheme();
        SwitchColors {
            checked_thumb_color: theme.on_primary,
            checked_track_color: theme.primary,
            unchecked_thumb_color: theme.on_surface,
            unchecked_track_color: theme.surface_variant,
        }
    }
}

/// Emits a switch with every part of its look given, as Compose's
/// `Switch(checked, onCheckedChange, modifier, enabled, colors)`. The switch fills the size its
/// `modifier` gives it, [`SwitchDefaults::WIDTH`] by [`SwitchDefaults::HEIGHT`] otherwise: a
/// fully rounded track with a round knob two pixels inside it, which slides to the end while
/// on. `on_checked_change` is called with the opposite of `checked` when it is clicked, unless
/// it is `None` (for a switch inside a larger control that answers the pointer) or the switch
/// is not `enabled`, which also draws it at 38% opacity.
#[track_caller]
pub fn switch_with(
    modifier: Modifier,
    checked: bool,
    on_checked_change: Option<Box<dyn Fn(bool)>>,
    enabled: bool,
    colors: SwitchColors,
) {
    const INSET: f32 = 2.0;
    let travel = animate_float_as_state(if checked { 1.0 } else { 0.0 }, tween(140));
    let (track, knob) = if checked {
        (colors.checked_track_color, colors.checked_thumb_color)
    } else {
        (colors.unchecked_track_color, colors.unchecked_thumb_color)
    };
    let mut modifier = modifier
        .size(SwitchDefaults::WIDTH, SwitchDefaults::HEIGHT)
        .semantics(Semantics::new(Role::Switch).toggled(checked));
    match (on_checked_change, enabled) {
        (Some(change), true) => modifier = modifier.clickable(move || change(!checked)),
        (_, false) => modifier = modifier.alpha(0.38),
        (None, true) => {}
    }
    let along = travel.get();
    canvas(modifier, move |canvas| {
        let (width, height) = (canvas.width(), canvas.height());
        canvas.rounded_rect(0.0, 0.0, width, height, height / 2.0, track);
        let side = (height - 2.0 * INSET).max(0.0);
        let x = INSET + (width - side - 2.0 * INSET).max(0.0) * along;
        canvas.rounded_rect(x, INSET, side, side, side / 2.0, knob);
    });
}

/// Emits a switch that is on or off and calls `on_change` with the opposite
/// value when clicked. The thumb slides and the track changes color.
#[track_caller]
pub fn switch(checked: bool, on_change: impl Fn(bool) + 'static) {
    let on_change = std::rc::Rc::new(on_change);
    let set = std::rc::Rc::clone(&on_change);
    let theme = color_scheme();
    let thumb = animate_float_as_state(if checked { 18.0 } else { 2.0 }, tween(140));
    let track = animate_color_as_state(
        if checked {
            theme.primary
        } else {
            theme.surface_variant
        },
        tween(140),
    );
    let knob = if checked {
        theme.on_primary
    } else {
        theme.on_surface
    };
    stack_aligned(
        Modifier::new()
            .size(36.0, 20.0)
            .semantics(
                Semantics::new(Role::Switch)
                    .toggled(checked)
                    .on_set_value(move |text| {
                        if let Some(on) = parse_toggle(&text) {
                            set(on);
                        }
                    }),
            )
            .clickable(move || on_change(!checked))
            .background_rounded(track.get(), 10.0),
        BoxAlignment::CenterStart,
        || {
            spacer(
                Modifier::new()
                    .offset(thumb.get(), 0.0)
                    .size(16.0, 16.0)
                    .background_rounded(knob, 8.0),
            );
        },
    );
}

/// Emits a radio button that is selected or not and runs `on_select` when clicked.
#[track_caller]
pub fn radio_button(selected: bool, on_select: impl Fn() + 'static) {
    let theme = color_scheme();
    let outline = if selected {
        theme.primary
    } else {
        theme.outline
    };
    stack_aligned(
        Modifier::new()
            .size(16.0, 16.0)
            .semantics(Semantics::new(Role::RadioButton).toggled(selected))
            .clickable(on_select)
            .border(1.0, outline, 8.0),
        BoxAlignment::Center,
        || {
            if selected {
                spacer(
                    Modifier::new()
                        .size(8.0, 8.0)
                        .background_rounded(theme.primary, 4.0),
                );
            }
        },
    );
}

/// Emits a slider `width` wide with `value` from 0 to 1, calling `on_change`
/// with the new value while it is dragged.
#[track_caller]
pub fn slider(width: f32, value: f32, on_change: impl Fn(f32) + 'static) {
    const THUMB: f32 = 16.0;
    let theme = color_scheme();
    let value = value.clamp(0.0, 1.0);
    let travel = (width - THUMB).max(1.0);
    let on_change = std::rc::Rc::new(on_change);
    let set = std::rc::Rc::clone(&on_change);
    stack_aligned(
        Modifier::new()
            .size(width, 24.0)
            .semantics(
                Semantics::new(Role::Slider)
                    .value(format!("{:.0}%", value * 100.0))
                    .on_set_value(move |text| {
                        if let Some(value) = parse_percent(&text) {
                            set(value);
                        }
                    }),
            )
            .on_drag(move |at| on_change(((at.x - THUMB / 2.0) / travel).clamp(0.0, 1.0))),
        BoxAlignment::CenterStart,
        || {
            spacer(
                Modifier::new()
                    .fill_max_width()
                    .height(6.0)
                    .background_rounded(theme.surface_variant, 3.0),
            );
            spacer(
                Modifier::new()
                    .width(value * travel + THUMB / 2.0)
                    .height(6.0)
                    .background_rounded(theme.primary, 3.0),
            );
            spacer(
                Modifier::new()
                    .offset(value * travel, 0.0)
                    .size(THUMB, THUMB)
                    .background_rounded(theme.background, THUMB / 2.0)
                    .border(1.5, theme.primary, THUMB / 2.0),
            );
        },
    );
}

/// Emits a slider `width` wide with two thumbs, for a range between `start`
/// and `end` (each 0 to 1, `start` first). `on_change` gets the new pair while
/// the thumb nearer to where the drag began is dragged.
#[track_caller]
pub fn range_slider(width: f32, start: f32, end: f32, on_change: impl Fn(f32, f32) + 'static) {
    const THUMB: f32 = 20.0;
    rustle_runtime::group(|| {
        let theme = color_scheme();
        let travel = (width - THUMB).max(1.0);
        let (start, end) = (start.clamp(0.0, 1.0), end.clamp(0.0, 1.0));
        // Which thumb the current drag moves: 0 not decided yet, 1 the start, 2 the end.
        let active = remember(|| std::rc::Rc::new(std::cell::Cell::new(0_u8))).get();
        let release = std::rc::Rc::clone(&active);

        stack_aligned(
            Modifier::new()
                .size(width, 28.0)
                .semantics(Semantics::new(Role::Slider).value(format!(
                    "{:.0}% to {:.0}%",
                    start * 100.0,
                    end * 100.0
                )))
                .on_drag_end(move || release.set(0))
                .on_drag(move |at| {
                    let value = ((at.x - THUMB / 2.0) / travel).clamp(0.0, 1.0);
                    if active.get() == 0 {
                        let nearer_start = (value - start).abs() <= (value - end).abs();
                        active.set(if nearer_start { 1 } else { 2 });
                    }
                    if active.get() == 1 {
                        on_change(value.min(end), end);
                    } else {
                        on_change(start, value.max(start));
                    }
                }),
            BoxAlignment::CenterStart,
            || {
                spacer(
                    Modifier::new()
                        .fill_max_width()
                        .height(4.0)
                        .background_rounded(theme.outline, 2.0),
                );
                spacer(
                    Modifier::new()
                        .offset(start * travel + THUMB / 2.0, 0.0)
                        .width((end - start) * travel)
                        .height(4.0)
                        .background(theme.primary),
                );
                for value in [start, end] {
                    spacer(
                        Modifier::new()
                            .offset(value * travel, 0.0)
                            .size(THUMB, THUMB)
                            .background_rounded(theme.primary, THUMB / 2.0)
                            .shadow(3.0, THUMB / 2.0),
                    );
                }
            },
        );
    });
}
