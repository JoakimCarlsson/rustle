//! Toggles and sliders.

use rustle_animation::{animate_color_as_state, animate_float_as_state, tween};
use rustle_foundation::{BoxAlignment, Modifier, Role, Semantics, spacer, stack_aligned};
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
