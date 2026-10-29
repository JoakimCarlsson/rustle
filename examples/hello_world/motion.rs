//! Everything that moves: easing curves, springs, staggered entrances, loaders,
//! hover effects, sliding tabs, swipe to dismiss, a carousel, drag and drop and
//! a few canvases driven by a clock.

use std::f32::consts::TAU;
use std::time::Duration;

use rustle::prelude::*;

use crate::ctx::Ctx;
use crate::pages::page_frame;
use crate::ui::{money, panel};

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

// ---------------------------------------------------------------------------
// Canvases driven by a clock

#[composable]
fn equalizer() {
    let theme = theme();
    let t = clock(5.0).get();
    let (strong, faint) = (
        theme.primary,
        Color::rgba(theme.primary.r, theme.primary.g, theme.primary.b, 70),
    );
    panel(
        "Live canvas",
        "Bars and waves redrawn every frame from a looping clock.",
        || {
            canvas(
                Modifier::new().fill_max_width().height(110.0),
                move |canvas| {
                    let count = 48;
                    let gap = 4.0;
                    let width = (canvas.width() - gap * (count as f32 - 1.0)) / count as f32;
                    for i in 0..count {
                        let phase = i as f32 * 0.33;
                        let wave = ((t * TAU * 2.0 + phase).sin() * 0.5 + 0.5)
                            * (0.55 + 0.45 * (t * TAU - phase * 0.4).cos());
                        let height = (6.0 + wave * (canvas.height() - 6.0)).min(canvas.height());
                        let color = if wave > 0.6 { strong } else { faint };
                        canvas.rounded_rect(
                            i as f32 * (width + gap),
                            (canvas.height() - height) / 2.0,
                            width,
                            height,
                            width / 2.0,
                            color,
                        );
                    }
                },
            );
            spacer(Modifier::new().height(12.0));
            waves(t);
        },
    );
}

fn waves(t: f32) {
    let theme = theme();
    let layers = [
        (theme.primary, 0.9, 1.0, 2.0),
        (theme.on_surface_muted, 0.6, 1.6, 1.5),
        (theme.outline, 0.4, 2.4, 1.5),
    ];
    canvas(
        Modifier::new().fill_max_width().height(90.0),
        move |canvas| {
            let (width, height) = (canvas.width(), canvas.height());
            for (index, (color, amplitude, frequency, stroke)) in layers.iter().enumerate() {
                let points: Vec<(f32, f32)> = (0..=(width as usize / 4))
                    .map(|step| {
                        let x = step as f32 * 4.0;
                        let angle = x / width * TAU * frequency + t * TAU * (index as f32 + 1.0);
                        (x, height / 2.0 + angle.sin() * amplitude * height * 0.38)
                    })
                    .collect();
                if index == 0 {
                    canvas.fill_below(
                        &points,
                        height,
                        Color::rgba(color.r, color.g, color.b, 50),
                        Color::rgba(color.r, color.g, color.b, 0),
                    );
                }
                canvas.polyline(&points, *stroke, *color);
            }
        },
    );
}

// ---------------------------------------------------------------------------
// Easing and springs

#[composable]
fn track(label: &str, spec: AnimationSpec, end: bool) {
    let theme = theme();
    let progress = animate_float_as_state(if end { 1.0 } else { 0.0 }, spec).get();
    let (line, dot) = (theme.outline, theme.primary);
    row_arranged(
        Modifier::new().fill_max_width().padding_xy(0.0, 3.0),
        Arrangement::SpacedBy(8.0),
        Alignment::Center,
        || {
            stack_with(Modifier::new().width(84.0), || muted_sized(label, 12.0));
            canvas(Modifier::new().weight(1.0).height(24.0), move |canvas| {
                let (left, right) = (10.0, canvas.width() - 10.0);
                let x = left + progress * (right - left);
                let middle = canvas.height() / 2.0;
                canvas.line((left, middle), (right, middle), 2.0, line);
                canvas.line((left, middle), (x, middle), 2.0, dot);
                canvas.circle(x, middle, 8.0, dot);
            });
        },
    );
}

#[composable]
fn easing_race() {
    let end = remember(|| false);
    panel(
        "Easing and springs",
        "The same trip, taken five ways. The spring overshoots.",
        || {
            let tracks = [
                ("Linear", linear(1100, Easing::Linear)),
                ("Ease in", linear(1100, Easing::EaseIn)),
                ("Ease out", linear(1100, Easing::EaseOut)),
                ("Ease in-out", linear(1100, Easing::EaseInOut)),
                ("Spring", spring()),
            ];
            for (index, (label, spec)) in tracks.into_iter().enumerate() {
                key(index, || track(label, spec, end.get()));
            }
            spacer(Modifier::new().height(10.0));
            let toggle = end.clone();
            replay_button("Run it", move || toggle.update(|end| *end = !*end));
        },
    );
}

// ---------------------------------------------------------------------------
// Staggered entrance

#[composable]
fn stagger_row(index: usize, run: u64, glyph: Icon, label: &str, detail: &str) {
    let theme = theme();
    let progress = remember_animatable(0.0_f32);
    let driven = progress.clone();
    launched_effect(run, move |scope| async move {
        driven.snap_to(0.0);
        scope.delay(Duration::from_millis(index as u64 * 90)).await;
        driven
            .animate_to(&scope, 1.0, linear(520, Easing::EaseOut))
            .await;
    });
    let shown = progress.value();
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .alpha(shown)
            .offset((1.0 - shown) * 32.0, 0.0)
            .background_rounded(theme.secondary, 10.0)
            .border(1.0, theme.outline, 10.0)
            .padding_xy(12.0, 10.0),
        Arrangement::SpacedBy(12.0),
        Alignment::Center,
        || {
            stack_aligned(
                Modifier::new()
                    .size(32.0, 32.0)
                    .background_rounded(theme.accent, 8.0),
                BoxAlignment::Center,
                || icon(glyph, 15.0, theme.on_surface),
            );
            column(|| {
                title(label, 13.0);
                muted_sized(detail, 12.0);
            });
        },
    );
    spacer(Modifier::new().height(8.0));
}

#[composable]
fn stagger() {
    let run = remember(|| 0_u64);
    panel(
        "Staggered entrance",
        "Each row waits a beat longer, then slides and fades in.",
        || {
            let rows = [
                (phosphor::FOLDER, "Projects", "4 in flight"),
                (phosphor::USERS, "Team", "3 members online"),
                (phosphor::CHART_BAR, "Reports", "Weekly summary is ready"),
                (phosphor::BELL, "Alerts", "Nothing needs you"),
                (phosphor::GEAR, "Settings", "All synced"),
            ];
            for (index, (glyph, label, detail)) in rows.into_iter().enumerate() {
                key(index, || {
                    stagger_row(index, run.get(), glyph, label, detail)
                });
            }
            let again = run.clone();
            replay_button("Replay", move || again.update(|n| *n += 1));
        },
    );
}

// ---------------------------------------------------------------------------
// Loaders

#[composable]
fn loader_cell(label: &str, content: impl FnOnce()) {
    column_arranged(
        Modifier::new().weight(1.0),
        Arrangement::SpacedBy(8.0),
        Alignment::Center,
        || {
            stack_aligned(
                Modifier::new().size(44.0, 36.0),
                BoxAlignment::Center,
                content,
            );
            muted_sized(label, 11.0);
        },
    );
}

#[composable]
fn bouncing_dots() {
    let theme = theme();
    let t = clock(1.1).get();
    row_arranged(
        Modifier::new(),
        Arrangement::SpacedBy(5.0),
        Alignment::Center,
        || {
            for i in 0..3 {
                let lift = (t * TAU - i as f32 * 0.9).sin().max(0.0);
                spacer(
                    Modifier::new()
                        .offset(0.0, -lift * 9.0)
                        .size(8.0, 8.0)
                        .background_rounded(theme.primary, 4.0),
                );
            }
        },
    );
}

#[composable]
fn pulse() {
    let theme = theme();
    let t = infinite_float(0.0, 1.0, Duration::from_millis(900), Easing::EaseInOut).get();
    stack_aligned(
        Modifier::new().size(30.0, 30.0),
        BoxAlignment::Center,
        || {
            spacer(
                Modifier::new()
                    .size(30.0, 30.0)
                    .alpha(1.0 - t)
                    .scale(0.4 + t * 0.6)
                    .background_rounded(theme.primary, 15.0),
            );
            spacer(
                Modifier::new()
                    .size(10.0, 10.0)
                    .background_rounded(theme.primary, 5.0),
            );
        },
    );
}

#[composable]
fn mini_bars() {
    let theme = theme();
    let t = clock(1.0).get();
    row_arranged(
        Modifier::new().height(28.0),
        Arrangement::SpacedBy(3.0),
        Alignment::Center,
        || {
            for i in 0..5 {
                let height = 8.0 + ((t * TAU + i as f32 * 0.8).sin() * 0.5 + 0.5) * 20.0;
                spacer(
                    Modifier::new()
                        .size(4.0, height)
                        .background_rounded(theme.primary, 2.0),
                );
            }
        },
    );
}

#[composable]
fn sweep_bar() {
    let theme = theme();
    let t = clock(1.5).get();
    let (track, fill) = (theme.accent, theme.primary);
    canvas(
        Modifier::new().fill_max_width().height(6.0),
        move |canvas| {
            let width = canvas.width();
            canvas.rounded_rect(0.0, 0.0, width, 6.0, 3.0, track);
            let length = width * 0.35;
            let x = -length + t * (width + length);
            canvas.push_clip(0.0, 0.0, width, 6.0);
            canvas.rounded_rect(x, 0.0, length, 6.0, 3.0, fill);
            canvas.pop_clip();
        },
    );
}

#[composable]
fn skeleton() {
    let theme = theme();
    let strength = infinite_float(0.35, 1.0, Duration::from_millis(900), Easing::EaseInOut).get();
    let shade = Color::rgba(
        theme.on_surface_muted.r,
        theme.on_surface_muted.g,
        theme.on_surface_muted.b,
        (strength * 60.0) as u8,
    );
    row_arranged(
        Modifier::new().fill_max_width(),
        Arrangement::SpacedBy(12.0),
        Alignment::Center,
        || {
            spacer(
                Modifier::new()
                    .size(40.0, 40.0)
                    .background_rounded(shade, 20.0),
            );
            column_with(Modifier::new().weight(1.0), || {
                spacer(
                    Modifier::new()
                        .fill_max_width()
                        .height(12.0)
                        .background_rounded(shade, 6.0),
                );
                spacer(Modifier::new().height(8.0));
                spacer(
                    Modifier::new()
                        .width(160.0)
                        .height(12.0)
                        .background_rounded(shade, 6.0),
                );
            });
        },
    );
}

#[composable]
fn loaders() {
    let theme = theme();
    let target = remember(|| 0.25_f32);
    let progress = animate_float_as_state(target.get(), linear(800, Easing::EaseInOut)).get();
    panel(
        "Loaders",
        "Spinners, pulses and skeletons that never stop moving.",
        || {
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpaceBetween,
                Alignment::Start,
                || {
                    loader_cell("Spinner", || {
                        circular_progress_indicator_indeterminate(28.0)
                    });
                    loader_cell("Dots", bouncing_dots);
                    loader_cell("Pulse", pulse);
                    loader_cell("Level", mini_bars);
                },
            );
            spacer(Modifier::new().height(16.0));
            sweep_bar();
            spacer(Modifier::new().height(18.0));
            skeleton();
            spacer(Modifier::new().height(18.0));
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpacedBy(16.0),
                Alignment::Center,
                || {
                    stack_aligned(
                        Modifier::new().size(84.0, 84.0),
                        BoxAlignment::Center,
                        || {
                            spacer(Modifier::new().size(84.0, 84.0).ring(
                                theme.accent,
                                8.0,
                                0.0,
                                1.0,
                            ));
                            spacer(Modifier::new().size(84.0, 84.0).ring(
                                theme.primary,
                                8.0,
                                0.0,
                                progress,
                            ));
                            title(&format!("{:.0}%", progress * 100.0), 16.0);
                        },
                    );
                    column_with(Modifier::new().weight(1.0), || {
                        muted_sized("Determinate ring, animated to each target.", 12.0);
                        spacer(Modifier::new().height(8.0));
                        row_arranged(
                            Modifier::new(),
                            Arrangement::SpacedBy(6.0),
                            Alignment::Center,
                            || {
                                for (label, value) in [("25%", 0.25), ("60%", 0.6), ("100%", 1.0)] {
                                    key(label, || {
                                        let target = target.clone();
                                        styled_button(
                                            Modifier::new(),
                                            ButtonStyle::Secondary,
                                            label,
                                            None,
                                            None,
                                            move || target.set(value),
                                        );
                                    });
                                }
                            },
                        );
                    });
                },
            );
        },
    );
}

// ---------------------------------------------------------------------------
// Numbers and bars that tween

#[composable]
fn counter_and_bars() {
    let theme = theme();
    let total = remember(|| 15_231.89_f64);
    let seed = remember(|| 7_u64);
    let shown = animate_float_as_state(total.get() as f32, linear(900, Easing::EaseOut)).get();

    let mut heights = Vec::new();
    for i in 0..12_u64 {
        let target = 0.18 + noise(seed.get() * 31 + i) * 0.82;
        heights.push(key(i, || animate_float_as_state(target, spring()).get()));
    }
    let (strong, faint) = (theme.primary, theme.accent);
    panel(
        "Tweened values",
        "Numbers and bars glide to their new value instead of jumping.",
        || {
            title(&money(f64::from(shown)), 28.0);
            muted_sized("Total revenue", 12.0);
            spacer(Modifier::new().height(12.0));
            canvas(
                Modifier::new().fill_max_width().height(96.0),
                move |canvas| {
                    let count = heights.len();
                    let gap = 8.0;
                    let width = (canvas.width() - gap * (count as f32 - 1.0)) / count as f32;
                    for (i, height) in heights.iter().enumerate() {
                        let h = (height * canvas.height()).clamp(2.0, canvas.height());
                        let x = i as f32 * (width + gap);
                        canvas.rounded_rect(x, 0.0, width, canvas.height(), 5.0, faint);
                        canvas.rounded_rect(x, canvas.height() - h, width, h, 5.0, strong);
                    }
                },
            );
            spacer(Modifier::new().height(12.0));
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(8.0),
                Alignment::Center,
                || {
                    let (total, seed) = (total.clone(), seed.clone());
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Default,
                        "Shuffle",
                        Some(phosphor::PULSE),
                        None,
                        move || {
                            seed.update(|s| *s += 1);
                            total.set(1_000.0 + f64::from(noise(seed.get() * 7)) * 90_000.0);
                        },
                    );
                },
            );
        },
    );
}

// ---------------------------------------------------------------------------
// Hover, press and click feedback

#[composable]
fn lift_card(glyph: Icon, heading: &str, detail: &str) {
    let theme = theme();
    let hovered = remember(|| false);
    let hover = hovered.clone();
    let lift = animate_float_as_state(if hovered.get() { 1.0 } else { 0.0 }, tween(200)).get();
    let fill = animate_color_as_state(
        if hovered.get() {
            theme.accent
        } else {
            theme.secondary
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
fn spring_switch() {
    let theme = theme();
    let on = remember(|| false);
    let toggle = on.clone();
    let x = animate_float_as_state(if on.get() { 20.0 } else { 0.0 }, spring()).get();
    let track = animate_color_as_state(
        if on.get() {
            theme.primary
        } else {
            theme.accent
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
fn bell_button() {
    let theme = theme();
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
fn pop_button() {
    let theme = theme();
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
            theme.success
        } else {
            theme.accent
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
fn feedback() {
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

// ---------------------------------------------------------------------------
// Tabs with a sliding indicator

#[composable]
fn sliding_tabs() {
    let theme = theme();
    const WIDTH: f32 = 96.0;
    let labels = ["Overview", "Analytics", "Reports"];
    let selected = remember(|| 0_usize);
    let x = animate_float_as_state(selected.get() as f32 * WIDTH, spring()).get();
    panel(
        "Sliding tabs",
        "The highlight springs to the tab you pick; the content crossfades.",
        || {
            stack_aligned(
                Modifier::new()
                    .background_rounded(theme.secondary, 10.0)
                    .border(1.0, theme.outline, 10.0)
                    .padding(3.0),
                BoxAlignment::CenterStart,
                || {
                    spacer(
                        Modifier::new()
                            .offset(x, 0.0)
                            .size(WIDTH, 30.0)
                            .shadow(3.0, 8.0)
                            .background_rounded(theme.accent, 8.0),
                    );
                    row_arranged(
                        Modifier::new(),
                        Arrangement::Start,
                        Alignment::Center,
                        || {
                            for (index, label) in labels.iter().enumerate() {
                                key(index, || {
                                    let pick = selected.clone();
                                    stack_aligned(
                                        Modifier::new()
                                            .size(WIDTH, 30.0)
                                            .semantics(
                                                Semantics::new(Role::Tab)
                                                    .label(label.to_string())
                                                    .toggled(selected.get() == index)
                                                    .merging_descendants(),
                                            )
                                            .clickable(move || pick.set(index)),
                                        BoxAlignment::Center,
                                        || {
                                            let color = if selected.get() == index {
                                                theme.on_surface
                                            } else {
                                                theme.on_surface_muted
                                            };
                                            text_styled(
                                                Modifier::new(),
                                                label.to_string(),
                                                LOCAL_TEXT_STYLE
                                                    .current()
                                                    .color(color)
                                                    .font_weight(FontWeight::MEDIUM),
                                            );
                                        },
                                    );
                                });
                            }
                        },
                    );
                },
            );
            spacer(Modifier::new().height(14.0));
            stack_with(Modifier::new().fill_max_width().height(64.0), || {
                crossfade(selected.get(), |index| {
                    column(|| match index {
                        0 => {
                            title("Welcome back", 15.0);
                            muted_sized("Everything at a glance, as of this morning.", 13.0);
                        }
                        1 => {
                            title("Traffic is up 12%", 15.0);
                            muted_sized("Most of it came from two referrals.", 13.0);
                        }
                        _ => {
                            title("3 reports ready", 15.0);
                            muted_sized("Share them, or schedule the next run.", 13.0);
                        }
                    });
                });
            });
        },
    );
}

// ---------------------------------------------------------------------------
// Accordion

#[composable]
fn accordion_item(heading: &str, body: &str, open: bool, on_toggle: impl Fn() + 'static) {
    let theme = theme();
    let turn = animate_float_as_state(if open { 180.0 } else { 0.0 }, tween(220)).get();
    column_with(Modifier::new().fill_max_width(), || {
        row_arranged(
            Modifier::new()
                .fill_max_width()
                .semantics(
                    Semantics::new(Role::Button)
                        .label(heading.to_string())
                        .toggled(open)
                        .merging_descendants(),
                )
                .clickable(on_toggle)
                .padding_xy(0.0, 12.0),
            Arrangement::SpaceBetween,
            Alignment::Center,
            || {
                title(heading, 14.0);
                stack_aligned(
                    Modifier::new().size(16.0, 16.0).rotate(turn),
                    BoxAlignment::Center,
                    || icon(phosphor::CARET_DOWN, 16.0, theme.on_surface_muted),
                );
            },
        );
        animated_visibility(open, || {
            column_with(
                Modifier::new()
                    .fill_max_width()
                    .padding_each(0.0, 0.0, 0.0, 12.0),
                || muted_sized(body, 13.0),
            );
        });
        spacer(
            Modifier::new()
                .fill_max_width()
                .height(1.0)
                .background(theme.outline),
        );
    });
}

#[composable]
fn accordion() {
    let open = remember(|| Some(0_usize));
    panel(
        "Accordion",
        "One section open at a time; the chevron turns as it opens.",
        || {
            let items = [
                (
                    "Is it accessible?",
                    "Yes. Every control carries a role and a label.",
                ),
                (
                    "Is it animated?",
                    "Yes. Sections expand and fade, and the chevron turns.",
                ),
                (
                    "Is it fast?",
                    "Only what changed is composed again, and the rest is skipped.",
                ),
            ];
            for (index, (heading, body)) in items.into_iter().enumerate() {
                key(index, || {
                    let state = open.clone();
                    accordion_item(heading, body, open.get() == Some(index), move || {
                        let next = if state.get() == Some(index) {
                            None
                        } else {
                            Some(index)
                        };
                        state.set(next);
                    });
                });
            }
        },
    );
}

// ---------------------------------------------------------------------------
// Carousel

#[composable]
fn carousel() {
    let pager = remember_pager_state();
    let auto = pager.clone();
    launched_effect((), move |scope| async move {
        loop {
            scope.delay(Duration::from_millis(3200)).await;
            auto.animate_to_page((auto.current_page() + 1) % 3);
        }
    });
    panel(
        "Carousel",
        "Drag to swipe, or wait: it advances by itself.",
        || {
            let slides = [
                (
                    "Ship faster",
                    "Compose, recompose, done.",
                    Color::rgb(79, 70, 229),
                    Color::rgb(168, 85, 247),
                ),
                (
                    "Look sharp",
                    "Hairlines, tokens and a calm palette.",
                    Color::rgb(14, 116, 144),
                    Color::rgb(52, 211, 153),
                ),
                (
                    "Stay smooth",
                    "Springs, tweens and clocks.",
                    Color::rgb(190, 18, 60),
                    Color::rgb(251, 146, 60),
                ),
            ];
            horizontal_pager(
                Modifier::new().fill_max_width().height(150.0),
                &pager,
                slides.len(),
                |index| {
                    let (heading, detail, from, to) = slides[index];
                    stack_aligned(
                        Modifier::new()
                            .fill_max_size()
                            .background_gradient(from, to, 35.0, 12.0)
                            .padding(20.0),
                        BoxAlignment::BottomStart,
                        || {
                            column(|| {
                                let white = Color::rgb(255, 255, 255);
                                text_styled(
                                    Modifier::new(),
                                    heading.to_string(),
                                    LOCAL_TEXT_STYLE
                                        .current()
                                        .color(white)
                                        .font_size(24.0)
                                        .font_weight(FontWeight::BOLD),
                                );
                                text_styled(
                                    Modifier::new(),
                                    detail.to_string(),
                                    LOCAL_TEXT_STYLE
                                        .current()
                                        .color(Color::rgba(255, 255, 255, 210)),
                                );
                            });
                        },
                    );
                },
            );
            spacer(Modifier::new().height(12.0));
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::Center,
                Alignment::Center,
                || page_indicator(slides.len(), pager.current_page()),
            );
        },
    );
}

// ---------------------------------------------------------------------------
// Swipe to dismiss

#[composable]
fn swipe_list(ctx: &Ctx) {
    let theme = theme();
    let start = || {
        vec![
            (0_usize, "Eddie Lake commented on Design"),
            (1, "Cover page is ready for review"),
            (2, "Weekly report was generated"),
            (3, "Sofia Davis joined the team"),
        ]
    };
    let notes = remember_state_list(start);
    panel(
        "Swipe to dismiss",
        "Drag a row sideways; past a third of its width it goes.",
        || {
            if notes.is_empty() {
                stack_aligned(
                    Modifier::new().fill_max_width().height(80.0),
                    BoxAlignment::Center,
                    || muted_sized("All caught up", 13.0),
                );
            }
            for (id, message) in notes.to_vec() {
                key(id, || {
                    let (notes, ctx) = (notes.clone(), ctx.clone());
                    swipe_to_dismiss(
                        move || {
                            notes.update(|list| list.retain(|(other, _)| *other != id));
                            ctx.say("Dismissed");
                        },
                        || {
                            row_arranged(
                                Modifier::new()
                                    .fill_max_width()
                                    .background_rounded(theme.secondary, 10.0)
                                    .border(1.0, theme.outline, 10.0)
                                    .padding_xy(14.0, 12.0),
                                Arrangement::SpacedBy(10.0),
                                Alignment::Center,
                                || {
                                    icon(phosphor::BELL, 15.0, theme.on_surface_muted);
                                    text(message.to_string());
                                },
                            );
                        },
                    );
                    spacer(Modifier::new().height(8.0));
                });
            }
            let again = notes.clone();
            replay_button("Restore", move || again.update(|list| *list = start()));
        },
    );
}

// ---------------------------------------------------------------------------
// Drag and drop

const LANES: [&str; 3] = ["To do", "In progress", "Done"];

#[composable]
fn kanban(ctx: &Ctx) {
    let theme = theme();
    let cards = remember_state_list(|| {
        vec![
            (0_usize, "Write the docs", 0_usize),
            (1, "Review the pull request", 0),
            (2, "Fix the flaky test", 1),
            (3, "Ship 0.1", 2),
        ]
    });
    panel(
        "Drag and drop",
        "Pick a card up and drop it on another column.",
        || {
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpacedBy(12.0),
                Alignment::Start,
                || {
                    for (lane, name) in LANES.iter().enumerate() {
                        key(lane, || {
                            let (cards, ctx) = (cards.clone(), ctx.clone());
                            let dropped = cards.clone();
                            column_with(
                                Modifier::new()
                                    .weight(1.0)
                                    .height_in(150.0, 400.0)
                                    .drop_target(move |payload| {
                                        let Some(id) = payload.downcast_ref::<usize>() else {
                                            return;
                                        };
                                        let list = dropped.to_vec();
                                        if let Some((_, label, from)) =
                                            list.iter().find(|(card, _, _)| card == id)
                                            && *from != lane
                                        {
                                            ctx.say(format!(
                                                "Moved \u{201c}{label}\u{201d} to {}",
                                                LANES[lane]
                                            ));
                                        }
                                        dropped.update(|list| {
                                            for card in list.iter_mut().filter(|c| c.0 == *id) {
                                                card.2 = lane;
                                            }
                                        });
                                    })
                                    .background_rounded(theme.secondary, 10.0)
                                    .border(1.0, theme.outline, 10.0)
                                    .padding(10.0),
                                || {
                                    row_arranged(
                                        Modifier::new().fill_max_width().padding_xy(2.0, 4.0),
                                        Arrangement::SpaceBetween,
                                        Alignment::Center,
                                        || {
                                            title(name, 13.0);
                                            let count = cards
                                                .to_vec()
                                                .iter()
                                                .filter(|c| c.2 == lane)
                                                .count();
                                            pill(&count.to_string(), BadgeStyle::Secondary);
                                        },
                                    );
                                    spacer(Modifier::new().height(6.0));
                                    for (id, label, _) in
                                        cards.to_vec().into_iter().filter(|c| c.2 == lane)
                                    {
                                        key(id, || {
                                            row_arranged(
                                                Modifier::new()
                                                    .fill_max_width()
                                                    .drag_source(id)
                                                    .background_rounded(theme.surface, 8.0)
                                                    .border(1.0, theme.outline, 8.0)
                                                    .padding_xy(10.0, 9.0),
                                                Arrangement::SpacedBy(8.0),
                                                Alignment::Center,
                                                || {
                                                    icon(
                                                        phosphor::DOTS_SIX_VERTICAL,
                                                        14.0,
                                                        theme.on_surface_muted,
                                                    );
                                                    text(label.to_string());
                                                },
                                            );
                                            spacer(Modifier::new().height(6.0));
                                        });
                                    }
                                },
                            );
                        });
                    }
                },
            );
        },
    );
}

// ---------------------------------------------------------------------------

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
