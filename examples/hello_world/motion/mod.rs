//! Everything that moves: easing curves, springs, staggered entrances, loaders,
//! hover effects, sliding tabs, swipe to dismiss, a carousel, drag and drop and
//! a few canvases driven by a clock.

mod accordion;
mod canvas;
mod carousel;
mod counter;
mod easing;
mod feedback;
mod kanban;
mod loaders;
mod stagger;
mod swipe;
mod tabs;

use std::time::Duration;

use rustle::prelude::*;

use crate::ctx::Ctx;
use crate::pages::page_frame;
use accordion::accordion;
use canvas::equalizer;
use carousel::carousel;
use counter::counter_and_bars;
use easing::easing_race;
use feedback::feedback;
use kanban::kanban;
use loaders::loaders;
use stagger::stagger;
use swipe::swipe_list;
use tabs::sliding_tabs;

/// A value that runs from 0 to 1 over `seconds`, again and again.
#[composable]
fn clock(seconds: f32) -> State<f32> {
    infinite_float_repeating(0.0, 1.0, Duration::from_secs_f32(seconds), Easing::Linear)
}

fn linear(milliseconds: u64, easing: Easing) -> AnimationSpec {
    AnimationSpec::Tween {
        duration: Duration::from_millis(milliseconds),
        easing,
    }
}

/// A small pseudo-random number from `seed`, from 0 to 1.
fn noise(seed: u64) -> f32 {
    let mixed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((mixed >> 33) % 10_000) as f32 / 10_000.0
}

#[composable]
fn replay_button(label: &str, on_click: impl Fn() + 'static) {
    styled_button(
        Modifier::new(),
        ButtonStyle::Outline,
        label,
        Some(phosphor::PULSE),
        None,
        on_click,
    );
}

#[composable]
pub fn motion_page(ctx: &Ctx) {
    page_frame(
        "Motion",
        "Springs, tweens, clocks and gestures. Everything here is live.",
        || {
            equalizer();
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpacedBy(16.0),
                Alignment::Start,
                || {
                    column_with(Modifier::new().weight(1.0), || {
                        easing_race();
                        stagger();
                        counter_and_bars();
                        swipe_list(ctx);
                    });
                    column_with(Modifier::new().weight(1.0), || {
                        loaders();
                        feedback();
                        sliding_tabs();
                        accordion();
                        carousel();
                    });
                },
            );
            kanban(ctx);
        },
    );
}
