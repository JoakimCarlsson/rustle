//! The backdrop (a synthwave horizon over a skyline) and the glitching header.

use rustle::prelude::*;

use crate::fx::*;
use crate::hud::status_pill;

/// Sun, skyline, a perspective grid sliding toward you, dust and scanlines.
#[composable]
pub fn backdrop_scene() {
    let u = clock(20.0).get();
    canvas(Modifier::new().fill_max_size(), move |canvas| {
        let (w, h) = (canvas.width(), canvas.height());
        let horizon = h * 0.56;
        let cx = w * 0.5;
        canvas.gradient(
            0.0,
            0.0,
            w,
            horizon,
            0.0,
            NIGHT,
            Color::rgb(58, 10, 74),
            90.0,
        );

        // The sun, with the stripes cut out of its lower half.
        let radius = (w * 0.13).clamp(110.0, 210.0);
        let sun_y = horizon - radius * 0.55;
        glow(canvas, cx, sun_y, radius * 2.4, PINK, 130);
        canvas.gradient(
            cx - radius,
            sun_y - radius,
            radius * 2.0,
            radius * 2.0,
            radius,
            YELLOW,
            PINK,
            90.0,
        );
        for stripe in 0..7 {
            let t = stripe as f32 / 7.0;
            let top = sun_y + radius * (0.05 + t * 0.9);
            let thick = 2.0 + t * 10.0;
            canvas.rect(cx - radius, top, radius * 2.0, thick, Color::rgb(46, 8, 66));
        }

        // A skyline of dark towers with a few lit windows.
        let towers = 34;
        let tower_w = w / towers as f32;
        for i in 0..towers {
            let height = 30.0 + noise(i * 13 + 5).powf(1.6) * h * 0.2;
            let x = i as f32 * tower_w;
            canvas.rect(
                x,
                horizon - height,
                tower_w - 2.0,
                height,
                Color::rgb(14, 6, 30),
            );
            canvas.rect(x, horizon - height, tower_w - 2.0, 1.0, alpha(CYAN, 70));
            for window in 0..6_u64 {
                let lit = noise(i * 31 + window * 7) > 0.55;
                let blink = (turns(u, 3.0 + (window % 3) as f32) + i as f32).sin() > -0.2;
                if lit && blink {
                    let wy = horizon - height + 8.0 + window as f32 * 9.0;
                    if wy < horizon - 4.0 {
                        canvas.rect(
                            x + 3.0 + (window % 2) as f32 * 5.0,
                            wy,
                            3.0,
                            3.0,
                            alpha(YELLOW, 150),
                        );
                    }
                }
            }
        }

        // The floor, and a grid in perspective.
        canvas.gradient(
            0.0,
            horizon,
            w,
            h - horizon,
            0.0,
            Color::rgb(30, 6, 54),
            NIGHT,
            90.0,
        );
        canvas.shadow(0.0, horizon - 2.0, w, 4.0, 2.0, 26.0, alpha(PINK, 150));
        canvas.rect(0.0, horizon, w, 2.0, PINK);
        let depth = h - horizon;
        for line in 0..14 {
            let t = ((line as f32 + u * 14.0 * 2.0) % 14.0) / 14.0;
            let y = horizon + depth * t * t;
            canvas.rect(
                0.0,
                y,
                w,
                1.0 + t * 1.5,
                alpha(CYAN, (24.0 + t * 110.0) as u8),
            );
        }
        for spoke in -16..=16 {
            let reach = spoke as f32 * w * 0.09;
            let color = if spoke % 4 == 0 { PINK } else { CYAN };
            canvas.line(
                (cx + reach * 0.04, horizon),
                (cx + reach, h),
                1.2,
                alpha(color, 70),
            );
        }

        // Drifting dust.
        for i in 0..46_u64 {
            let x = noise(i * 3) * w;
            let rise = (noise(i * 5) * 2.0 + u * (1.0 + (noise(i * 7) * 3.0).floor())) % 1.0;
            let y = h * (1.0 - rise);
            let tint = if i % 3 == 0 { PINK } else { CYAN };
            canvas.circle(
                x + (turns(u, 2.0) + i as f32).sin() * 12.0,
                y,
                1.3,
                alpha(tint, (rise * 160.0) as u8),
            );
        }

        // Scanlines and a vignette.
        let mut y = 0.0;
        while y < h {
            canvas.rect(0.0, y, w, 1.0, Color::rgba(0, 0, 0, 38));
            y += 3.0;
        }
        let edge = Color::rgba(0, 0, 0, 150);
        canvas.gradient(0.0, 0.0, w * 0.14, h, 0.0, edge, alpha(edge, 0), 0.0);
        canvas.gradient(w * 0.86, 0.0, w * 0.14, h, 0.0, alpha(edge, 0), edge, 0.0);
    });
}

/// The title with chromatic aberration that tears in short bursts, over a status line.
#[composable]
pub fn header() {
    let u = clock(60.0).get();
    let seconds = (u * 60.0) as u32;
    let burst = (u * 23.0 + 0.5).fract() < 0.04;
    let tear = if burst {
        3.0 + noise((u * 600.0) as u64) * 6.0
    } else {
        1.2
    };
    let jitter = if burst {
        (noise((u * 900.0) as u64) - 0.5) * 6.0
    } else {
        0.0
    };
    row_arranged(
        Modifier::new().fill_max_width().padding_xy(0.0, 4.0),
        Arrangement::SpaceBetween,
        Alignment::Center,
        || {
            column(|| {
                stack_with(Modifier::new(), || {
                    label(
                        Modifier::new().offset(-tear, jitter),
                        "NIGHT//CITY",
                        34.0,
                        800,
                        alpha(PINK, 230),
                    );
                    label(
                        Modifier::new().offset(tear, -jitter),
                        "NIGHT//CITY",
                        34.0,
                        800,
                        alpha(CYAN, 230),
                    );
                    label(Modifier::new(), "NIGHT//CITY", 34.0, 800, INK);
                });
                mono(
                    Modifier::new(),
                    "NETWATCH OPS  //  SECTOR 07  //  UPLINK STABLE",
                    11.0,
                    500,
                    DIM,
                );
            });
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(14.0),
                Alignment::Center,
                || {
                    mono(
                        Modifier::new(),
                        &format!("T+ 00:{:02}:{:02}", seconds / 60, seconds % 60),
                        14.0,
                        600,
                        CYAN,
                    );
                    status_pill("ICE ONLINE", GREEN);
                    status_pill("TRACE 12%", YELLOW);
                },
            );
        },
    );
}
