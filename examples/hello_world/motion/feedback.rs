//! Hover and click feedback.

use std::f32::consts::TAU;

use rustle::prelude::*;

use crate::extended::extended_colors;

use super::linear;
use crate::ui::panel;

#[composable]
pub(super) fn lift_card(glyph: Icon, heading: &str, detail: &str) {
    let theme = color_scheme();
    let hovered = remember(|| false);
    let hover = hovered.clone();
    let lift = animate_float_as_state(if hovered.get() { 1.0 } else { 0.0 }, tween(200)).get();
    let fill = animate_color_as_state(
        if hovered.get() {
            theme.surface_variant
        } else {
            theme.secondary_container
        },
        tween(200),
    )
    .get();
    column_with(
        Modifier::new()
            .weight(1.0)
            .on_hover(move |over| hover.set(over))
            .graphics_layer(1.0, 0.0, 1.0 + 0.05 * lift)
            .shadow(lift * 14.0, 10.0)
            .background_rounded(fill, 10.0)
            .border(1.0, theme.outline, 10.0)
            .padding(14.0),
        || {
            stack_aligned(
                Modifier::new().size(34.0, 34.0).rotate(lift * 18.0),
                BoxAlignment::Center,
                || icon(glyph, 20.0 + lift * 4.0, theme.on_surface),
            );
            spacer(Modifier::new().height(10.0));
            title(heading, 13.0);
            muted_sized(detail, 12.0);
        },
    );
}

#[composable]
pub(super) fn spring_switch() {
    let theme = color_scheme();
    let on = remember(|| false);
    let toggle = on.clone();
    let x = animate_float_as_state(if on.get() { 20.0 } else { 0.0 }, spring()).get();
    let track = animate_color_as_state(
        if on.get() {
            theme.primary
        } else {
            theme.surface_variant
        },
        tween(180),
    )
    .get();
    stack_aligned(
        Modifier::new()
            .size(48.0, 28.0)
            .semantics(Semantics::new(Role::Switch).toggled(on.get()))
            .clickable(move || toggle.update(|v| *v = !*v))
            .background_rounded(track, 14.0)
            .padding(3.0),
        BoxAlignment::CenterStart,
        || {
            spacer(
                Modifier::new()
                    .offset(x, 0.0)
                    .size(22.0, 22.0)
                    .background_rounded(
                        if on.get() {
                            theme.on_primary
                        } else {
                            theme.on_surface
                        },
                        11.0,
                    ),
            );
        },
    );
}

#[composable]
pub(super) fn bell_button() {
    let theme = color_scheme();
    let rings = remember(|| 0_u64);
    let angle = remember_animatable(0.0_f32);
    let driven = angle.clone();
    let count = rings.get();
    launched_effect(count, move |scope| async move {
        if count == 0 {
            return;
        }
        let shake = driven.clone();
        animate_value(&scope, 0.0, 1.0, linear(900, Easing::Linear), move |p| {
            shake.snap_to((p * TAU * 4.0).sin() * 28.0 * (1.0 - p));
        })
        .await;
    });
    let ring = rings.clone();
    stack_aligned(
        Modifier::new()
            .size(44.0, 44.0)
            .semantics(Semantics::new(Role::Button).label("Ring the bell".to_string()))
            .clickable(move || ring.update(|n| *n += 1))
            .border(1.0, theme.outline, 22.0),
        BoxAlignment::Center,
        || {
            stack_aligned(
                Modifier::new().size(20.0, 20.0).rotate(angle.value()),
                BoxAlignment::Center,
                || icon(phosphor::BELL, 20.0, theme.on_surface),
            );
        },
    );
}

#[composable]
pub(super) fn pop_button() {
    let theme = color_scheme();
    let done = remember(|| false);
    let scale = remember_animatable(1.0_f32);
    let driven = scale.clone();
    let flag = done.clone();
    launched_effect(done.get(), move |scope| async move {
        driven.snap_to(0.5);
        driven.animate_to(&scope, 1.0, spring()).await;
        let _ = &flag;
    });
    let toggle = done.clone();
    let fill = animate_color_as_state(
        if done.get() {
            extended_colors().success
        } else {
            theme.surface_variant
        },
        tween(200),
    )
    .get();
    stack_aligned(
        Modifier::new()
            .size(44.0, 44.0)
            .semantics(Semantics::new(Role::Button).label("Mark done".to_string()))
            .clickable(move || toggle.update(|v| *v = !*v))
            .graphics_layer(1.0, 0.0, scale.value())
            .background_rounded(fill, 22.0),
        BoxAlignment::Center,
        || {
            if done.get() {
                icon(phosphor::CHECK, 22.0, theme.background);
            } else {
                icon(phosphor::PLUS, 20.0, theme.on_surface);
            }
        },
    );
}

#[composable]
pub(super) fn feedback() {
    panel(
        "Hover and click feedback",
        "Cards lift, icons turn, switches spring, the bell rings.",
        || {
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpacedBy(10.0),
                Alignment::Start,
                || {
                    lift_card(phosphor::STACK, "Layers", "Hover me");
                    lift_card(phosphor::GEAR, "Settings", "And me");
                    lift_card(phosphor::WALLET, "Wallet", "Me too");
                },
            );
            spacer(Modifier::new().height(20.0));
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpacedBy(20.0),
                Alignment::Center,
                || {
                    spring_switch();
                    bell_button();
                    pop_button();
                    muted_sized("Click them", 12.0);
                },
            );
        },
    );
}
