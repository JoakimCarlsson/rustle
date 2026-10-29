//! Live data: a streaming chart, tick-ring dials and a breakdown.

use std::f32::consts::{FRAC_PI_2, TAU};
use std::time::Duration;

use rustle::prelude::*;

use crate::fx::*;
use crate::hud::panel;

fn chart_style() -> ChartStyle {
    ChartStyle {
        grid: alpha(CYAN, 16),
        label: DIM,
        surface: Color::rgb(11, 6, 26),
        ..ChartStyle::default()
    }
}

/// Two series that scroll left as samples arrive, with a readout under the pointer.
#[composable]
pub fn stream_panel() {
    const SAMPLES: usize = 72;
    let tick = remember(|| 0_u64);
    let load = remember(|| wander(SAMPLES, 1.0, 620.0, 180.0));
    let echo = remember(|| wander(SAMPLES, 4.0, 300.0, 90.0));
    {
        let (tick, load, echo) = (tick.clone(), load.clone(), echo.clone());
        launched_effect((), move |scope| async move {
            loop {
                scope.delay(Duration::from_millis(120)).await;
                let n = tick.get() + 1;
                tick.set(n);
                let t = n as f32;
                let next = 620.0
                    + 180.0 * (0.55 * (t * 0.11).sin() + 0.3 * (t * 0.037).sin())
                    + (noise(n) - 0.5) * 80.0;
                let slow = 300.0 + 90.0 * (0.5 * (t * 0.07).cos()) + (noise(n * 7) - 0.5) * 40.0;
                load.update(|values| {
                    values.remove(0);
                    values.push(next);
                });
                echo.update(|values| {
                    values.remove(0);
                    values.push(slow);
                });
            }
        });
    }
    let (hover, hovered) = chart_hover(SAMPLES, 0.0, 0.0);
    let marker = hovered.map(|i| i.min(SAMPLES - 1));
    let (load, echo) = (load.get(), echo.get());
    let current = marker.map_or(load[SAMPLES - 1], |m| load[m]);
    panel(
        Modifier::new().weight(2.0),
        CYAN,
        "NET//01",
        "Throughput",
        "Packets per second, streaming live",
        || {
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                Alignment::Center,
                || {
                    mono(Modifier::new(), &format!("{current:.0}"), 40.0, 700, CYAN);
                    spacer(Modifier::new().width(10.0));
                    mono(Modifier::new(), "PKT/S", 12.0, 500, DIM);
                },
            );
            spacer(Modifier::new().height(8.0));
            stack_aligned(
                Modifier::new().fill_max_width().height(220.0).chain(hover),
                BoxAlignment::TopStart,
                || {
                    area_chart(
                        Modifier::new().fill_max_size(),
                        vec![
                            Series::new(echo, PINK).filled().thickness(2.0),
                            Series::new(load, CYAN).filled().thickness(2.5),
                        ],
                        vec![String::new(); SAMPLES],
                        1,
                        marker,
                        chart_style(),
                    );
                },
            );
        },
    );
}

/// A ring of ticks that light up to `target` percent, with the number in the middle.
#[composable]
fn dial(name: &'static str, target: f32, color: Color) {
    let level = animate_float_as_state(target, tween(1200)).get();
    let fill = (level / 100.0).clamp(0.0, 1.0);
    let reading = format!("{level:.0}");
    column_arranged(
        Modifier::new().weight(1.0),
        Arrangement::Start,
        Alignment::Center,
        || {
            canvas(
                Modifier::new().fill_max_width().height(104.0),
                move |canvas| {
                    let (x, y) = (canvas.width() / 2.0, canvas.height() / 2.0);
                    glow(canvas, x, y, 56.0, color, (fill * 90.0) as u8 + 10);
                    let ticks = 44;
                    for i in 0..ticks {
                        let f = i as f32 / ticks as f32;
                        let angle = f * TAU - FRAC_PI_2;
                        let lit = f < fill;
                        let (inner, outer) = (if lit { 36.0 } else { 40.0 }, 47.0);
                        canvas.line(
                            (x + angle.cos() * inner, y + angle.sin() * inner),
                            (x + angle.cos() * outer, y + angle.sin() * outer),
                            2.2,
                            if lit { color } else { alpha(color, 36) },
                        );
                    }
                    canvas.text(
                        x - 40.0,
                        y - 10.0,
                        80.0,
                        &reading,
                        20.0,
                        700,
                        INK,
                        TextAlign::Center,
                    );
                },
            );
            mono(Modifier::new(), name, 10.0, 600, DIM);
        },
    );
}

/// Six dials that settle on new readings every few seconds.
#[composable]
pub fn dials_panel() {
    let step = remember(|| 0_u64);
    {
        let step = step.clone();
        launched_effect((), move |scope| async move {
            loop {
                scope.delay(Duration::from_millis(2600)).await;
                step.set(step.get() + 1);
            }
        });
    }
    let reading = |lane: u64, low: f32, span: f32| low + noise(step.get() * 3 + lane) * span;
    panel(
        Modifier::new().weight(1.0),
        PINK,
        "SYS//02",
        "Cyberdeck",
        "Load across the rig",
        || {
            row_with(Modifier::new().fill_max_width(), || {
                dial("CPU", reading(1, 30.0, 60.0), CYAN);
                dial("RAM", reading(2, 40.0, 50.0), PURPLE);
                dial("ICE", reading(3, 15.0, 70.0), PINK);
            });
            spacer(Modifier::new().height(10.0));
            row_with(Modifier::new().fill_max_width(), || {
                dial("NET", reading(4, 20.0, 70.0), GREEN);
                dial("GPU", reading(5, 35.0, 60.0), YELLOW);
                dial("HEAT", reading(6, 50.0, 45.0), RED);
            });
        },
    );
}

/// A donut of traffic by sector next to a bar chart by hour.
#[composable]
pub fn breakdown_panel() {
    let (hover, hovered) = bar_hover(12);
    panel(
        Modifier::new().weight(1.0),
        GREEN,
        "TRF//06",
        "Traffic",
        "By sector, and by hour of the cycle",
        || {
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpaceBetween,
                Alignment::Center,
                || {
                    donut_chart(
                        Modifier::new().width(112.0).height(112.0),
                        vec![(0.46, CYAN), (0.28, PURPLE), (0.17, PINK), (0.09, YELLOW)],
                        13.0,
                        alpha(INK, 14),
                    );
                    column(|| {
                        for (name, share, color) in [
                            ("CORPO", "46%", CYAN),
                            ("STREET", "28%", PURPLE),
                            ("NOMAD", "17%", PINK),
                            ("OTHER", "9%", YELLOW),
                        ] {
                            key(name, || {
                                row_arranged(
                                    Modifier::new().width(120.0).padding_xy(0.0, 4.0),
                                    Arrangement::SpaceBetween,
                                    Alignment::Center,
                                    || {
                                        mono(Modifier::new(), name, 11.0, 400, DIM);
                                        mono(Modifier::new(), share, 11.0, 700, color);
                                    },
                                );
                            });
                        }
                    });
                },
            );
            spacer(Modifier::new().height(16.0));
            bar_chart(
                Modifier::new().fill_max_width().height(120.0).chain(hover),
                wander(12, 2.0, 55.0, 30.0),
                (0..12).map(|h| format!("{:02}", h * 2)).collect(),
                CYAN,
                alpha(PURPLE, 150),
                hovered.map(|index| (index, PINK)),
                true,
                chart_style(),
            );
        },
    );
}
