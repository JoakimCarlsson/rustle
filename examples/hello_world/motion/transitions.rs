//! Transitions, keyframes, repeats, decay, animated content, animated size,
//! path morphing and a shared element.

use rustle::prelude::*;
use rustle::{FillRule, Path, PathCommand, animate_as_state};

use crate::ui::panel;

const TRIANGLE: &[PathCommand] = &[
    PathCommand::MoveTo(50.0, 8.0),
    PathCommand::LineTo(92.0, 88.0),
    PathCommand::LineTo(8.0, 88.0),
    PathCommand::Close,
];

const SQUARE: &[PathCommand] = &[
    PathCommand::MoveTo(12.0, 12.0),
    PathCommand::LineTo(88.0, 12.0),
    PathCommand::LineTo(88.0, 88.0),
    PathCommand::Close,
];

const BOUNCE: &[Keyframe] = &[
    Keyframe::new(0.3, 0.9).with_easing(Easing::EaseOut),
    Keyframe::new(0.6, 0.5),
    Keyframe::new(0.8, 0.95),
];

#[composable]
fn labelled(label: &str, content: impl FnOnce()) {
    column_with(Modifier::new().padding_xy(0.0, 6.0), || {
        muted_sized(label, 12.0);
        spacer(Modifier::new().height(6.0));
        content();
    });
}

#[composable]
fn transition_card() {
    let theme = color_scheme();
    let selected = remember(|| false);
    let transition = update_transition(selected.get());
    let color = transition.animate_color(
        |_, _| tween(300),
        |on| if *on { theme.primary } else { theme.outline },
    );
    let scale = transition.animate_float(|_, _| spring(), |on| if *on { 1.2 } else { 1.0 });
    let toggle = selected.clone();
    labelled("update_transition: color and scale off one state", || {
        spacer(
            Modifier::new()
                .size(56.0, 56.0)
                .scale(scale.get())
                .background_rounded(color.get(), 12.0)
                .clickable(move || toggle.update(|on| *on = !*on)),
        );
    });
}

#[composable]
fn keyframe_dot() {
    let theme = color_scheme();
    let travel = remember_animatable(0.0_f32);
    let restart = remember(|| 0_u32);
    let driven = travel.clone();
    launched_effect(restart.get(), move |scope| async move {
        driven.snap_to(0.0);
        driven
            .animate_to(&scope, 1.0, keyframes(1200, BOUNCE))
            .await;
        let pulse = repeatable(3, tween(200), RepeatMode::Reverse);
        driven.animate_to(&scope, 0.5, pulse).await;
    });
    let again = restart.clone();
    labelled(
        "keyframes, then a repeatable reverse (click to replay)",
        || {
            canvas(
                Modifier::new()
                    .fill_max_width()
                    .height(40.0)
                    .clickable(move || again.update(|count| *count += 1)),
                {
                    let at = travel.value();
                    move |canvas| {
                        let x = 12.0 + at * (canvas.width() - 24.0);
                        canvas.circle(x, canvas.height() / 2.0, 10.0, theme.primary);
                    }
                },
            );
        },
    );
}

#[composable]
fn fling() {
    let theme = color_scheme();
    let position = remember_animatable(0.0_f32);
    let drag = position.clone();
    let last = remember(|| 0.0_f32);
    let velocity = remember(|| 0.0_f32);
    let throw = remember(|| 0_u32);
    let driven = position.clone();
    let speed = velocity.clone();
    let thrown = throw.clone();
    launched_effect(throw.get(), move |scope| async move {
        if thrown.get() > 0 {
            let bounded = DecaySpec::default().bounded(0.0, 300.0);
            driven.animate_decay(&scope, speed.get(), bounded).await;
        }
    });
    let (last_x, speed_x, last_end) = (last.clone(), velocity.clone(), last.clone());
    let again = throw.clone();
    labelled("animate_decay: drag and let go to fling", || {
        canvas(
            Modifier::new()
                .fill_max_width()
                .height(40.0)
                .on_drag(move |at| {
                    let moved = at.x - last_x.get();
                    last_x.set(at.x);
                    speed_x.set(moved * 60.0);
                    drag.snap_to((drag.value() + moved).clamp(0.0, 300.0));
                })
                .on_drag_end(move || {
                    last_end.set(0.0);
                    again.update(|count| *count += 1);
                }),
            {
                let at = position.value();
                move |canvas| {
                    canvas.circle(12.0 + at, canvas.height() / 2.0, 10.0, theme.primary);
                }
            },
        );
    });
}

#[composable]
fn counter_content() {
    let count = remember(|| 0_i32);
    let (up, down) = (count.clone(), count.clone());
    labelled("animated_content: slides by direction", || {
        row_arranged(
            Modifier::new(),
            Arrangement::SpacedBy(12.0),
            Alignment::Center,
            || {
                text_button("-", move || down.update(|n| *n -= 1));
                animated_content(
                    count.get(),
                    |from, to| {
                        let sign = if to > from { 1.0 } else { -1.0 };
                        ContentTransform::new(
                            EnterTransition::slide_in_vertically(sign)
                                .and(EnterTransition::fade_in(0.0)),
                            ExitTransition::slide_out_vertically(-sign)
                                .and(ExitTransition::fade_out(0.0)),
                        )
                        .with_size_transform(false)
                    },
                    |value| {
                        stack_with(Modifier::new().size(40.0, 28.0), || {
                            text_with(Modifier::new(), value.to_string());
                        });
                    },
                );
                text_button("+", move || up.update(|n| *n += 1));
            },
        );
    });
}

#[composable]
fn expanding() {
    let open = remember(|| false);
    let toggle = open.clone();
    labelled("animate_content_size", || {
        column_with(Modifier::new().fill_max_width(), || {
            replay_button(if open.get() { "Collapse" } else { "Expand" }, move || {
                toggle.update(|on| *on = !*on)
            });
            animate_content_size(Modifier::new(), tween(300), || {
                column_with(Modifier::new(), || {
                    muted_sized("The area glides to the size of what is inside it.", 13.0);
                    if open.get() {
                        muted_sized("More lines make it grow,", 13.0);
                        muted_sized("and fewer let it shrink back.", 13.0);
                    }
                });
            });
        });
    });
}

#[composable]
fn morph() {
    let theme = color_scheme();
    let flipped = remember(|| false);
    let toggle = flipped.clone();
    let from = Path::from_static(TRIANGLE, 100.0, 100.0, FillRule::NonZero);
    let to = Path::from_static(SQUARE, 100.0, 100.0, FillRule::NonZero);
    let target = if flipped.get() {
        to.clone()
    } else {
        from.clone()
    };
    let shape = animate_as_state(target, tween(500));
    labelled("Path morphing (click)", || {
        canvas(
            Modifier::new()
                .size(80.0, 80.0)
                .clickable(move || toggle.update(|on| *on = !*on)),
            {
                let shape = shape.get();
                move |canvas| canvas.path(&shape, 0.0, 0.0, 80.0, 80.0, theme.primary)
            },
        );
    });
}

#[composable]
fn shared() {
    let theme = color_scheme();
    let left = remember(|| true);
    let elements = remember_shared_elements();
    let toggle = left.clone();
    labelled("shared_element: glides between two places", || {
        row_arranged(
            Modifier::new()
                .fill_max_width()
                .clickable(move || toggle.update(|on| *on = !*on)),
            Arrangement::SpaceBetween,
            Alignment::Center,
            || {
                let badge = || {
                    shared_element(&elements, "badge", Modifier::new(), tween(400), || {
                        spacer(
                            Modifier::new()
                                .size(32.0, 32.0)
                                .background_rounded(theme.primary, 16.0),
                        );
                    });
                };
                stack_with(Modifier::new().size(40.0, 40.0), || {
                    if left.get() {
                        badge();
                    }
                });
                stack_with(Modifier::new().size(40.0, 40.0), || {
                    if !left.get() {
                        badge();
                    }
                });
            },
        );
    });
}

#[composable]
pub(super) fn transitions() {
    panel(
        "Transitions",
        "update_transition, keyframes, repeats, decay, animated content and size, morphing and shared elements.",
        || {
            transition_card();
            keyframe_dot();
            fling();
            counter_content();
            expanding();
            morph();
            shared();
        },
    );
}

use super::replay_button;
