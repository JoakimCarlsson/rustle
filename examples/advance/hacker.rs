//! The HUD pieces: radar, target lock, code rain, a terminal, a scope and a ticker.

use std::f32::consts::{PI, TAU};
use std::time::Duration;

use rustle::prelude::*;

use crate::fx::*;
use crate::hud::panel;

/// A sweep that lights up blips as it passes and lets them fade.
#[composable]
pub fn radar_panel() {
    let u = clock(6.0).get();
    panel(
        Modifier::new().weight(1.0),
        CYAN,
        "RDR//08",
        "Proximity radar",
        "Hostiles within 400 metres",
        || {
            canvas(
                Modifier::new().fill_max_width().height(200.0),
                move |canvas| {
                    let (cx, cy) = (canvas.width() / 2.0, canvas.height() / 2.0);
                    let radius = canvas.height() / 2.0 - 6.0;
                    glow(canvas, cx, cy, radius * 1.2, CYAN, 36);
                    for ring in 1..=4 {
                        canvas.stroke_circle(
                            cx,
                            cy,
                            radius * ring as f32 / 4.0,
                            1.0,
                            alpha(CYAN, 55),
                        );
                    }
                    canvas.line((cx - radius, cy), (cx + radius, cy), 1.0, alpha(CYAN, 45));
                    canvas.line((cx, cy - radius), (cx, cy + radius), 1.0, alpha(CYAN, 45));
                    let sweep = turns(u, 1.0);
                    for step in 0..44 {
                        let angle = sweep - step as f32 * 0.028;
                        let fade = (1.0 - step as f32 / 44.0).powi(2);
                        canvas.line(
                            (cx, cy),
                            (cx + angle.cos() * radius, cy + angle.sin() * radius),
                            3.0,
                            alpha(CYAN, (fade * 90.0) as u8),
                        );
                    }
                    canvas.line(
                        (cx, cy),
                        (cx + sweep.cos() * radius, cy + sweep.sin() * radius),
                        2.0,
                        CYAN,
                    );
                    for blip in 0..9_u64 {
                        let angle = noise(blip * 3 + 1) * TAU;
                        let distance = 0.25 + noise(blip * 5 + 2) * 0.7;
                        let behind = (sweep - angle).rem_euclid(TAU) / TAU;
                        let bright = (1.0 - behind).powi(3);
                        let (x, y) = (
                            cx + angle.cos() * radius * distance,
                            cy + angle.sin() * radius * distance,
                        );
                        glow(canvas, x, y, 16.0, PINK, (bright * 200.0) as u8);
                        canvas.circle(
                            x,
                            y,
                            2.0 + bright * 2.0,
                            alpha(PINK, (60.0 + bright * 195.0) as u8),
                        );
                    }
                },
            );
        },
    );
}

/// Rotating arcs and a crosshair that narrows to a lock.
#[composable]
pub fn reticle_panel() {
    let u = clock(10.0).get();
    let lock = 0.5 + 0.5 * turns(u, 1.0).sin();
    let reading = format!("LOCK {:02.0}%", 40.0 + lock * 59.0);
    panel(
        Modifier::new().weight(1.0),
        RED,
        "TGT//09",
        "Target lock",
        "Auto-aim calibrating",
        || {
            canvas(
                Modifier::new().fill_max_width().height(200.0),
                move |canvas| {
                    let (cx, cy) = (canvas.width() / 2.0, canvas.height() / 2.0);
                    let rings = [
                        (78.0, 4.0, 0.22, 1.0, RED),
                        (66.0, 2.0, 0.55, -2.0, PINK),
                        (54.0, 3.0, 0.18, 3.0, YELLOW),
                        (40.0, 2.0, 0.7, -1.0, RED),
                    ];
                    glow(canvas, cx, cy, 90.0, RED, 40 + (lock * 50.0) as u8);
                    for (radius, thick, sweep, speed, color) in rings {
                        canvas.stroke_circle(cx, cy, radius, 1.0, alpha(color, 30));
                        for part in 0..3 {
                            canvas.arc(
                                cx,
                                cy,
                                radius,
                                thick,
                                turns(u, speed).to_degrees() + part as f32 * 120.0,
                                sweep * 0.33,
                                color,
                            );
                        }
                    }
                    let gap = 24.0 - lock * 8.0;
                    for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                        canvas.line(
                            (cx + dx * gap, cy + dy * gap),
                            (cx + dx * 96.0, cy + dy * 96.0),
                            1.5,
                            alpha(RED, 200),
                        );
                    }
                    canvas.text(
                        cx - 50.0,
                        cy - 7.0,
                        100.0,
                        &reading,
                        12.0,
                        700,
                        INK,
                        TextAlign::Center,
                    );
                    for tick in 0..60 {
                        let angle = tick as f32 / 60.0 * TAU;
                        let long = if tick % 5 == 0 { 9.0 } else { 4.0 };
                        canvas.line(
                            (cx + angle.cos() * 90.0, cy + angle.sin() * 90.0),
                            (
                                cx + angle.cos() * (90.0 + long),
                                cy + angle.sin() * (90.0 + long),
                            ),
                            1.0,
                            alpha(RED, 120),
                        );
                    }
                },
            );
        },
    );
}

/// Columns of falling glyphs with a bright head and a fading tail.
#[composable]
pub fn rain_panel() {
    let u = clock(20.0).get();
    panel(
        Modifier::new().weight(1.0),
        GREEN,
        "ICE//10",
        "Code rain",
        "Intrusion countermeasures cycling",
        || {
            canvas(
                Modifier::new().fill_max_width().height(190.0),
                move |canvas| {
                    const GLYPHS: &[u8] = b"01<>/\\|=+*#$%&@ABCDEF";
                    let (width, height) = (canvas.width(), canvas.height());
                    let columns = 26;
                    let pitch = width / columns as f32;
                    let cell = 13.0;
                    let trail = 11;
                    canvas.push_clip(0.0, 0.0, width, height);
                    for column in 0..columns as u64 {
                        let loops = 1.0 + (noise(column * 7) * 3.0).floor();
                        let head = ((u * loops + noise(column * 3)) % 1.0)
                            * (height + cell * trail as f32);
                        for step in 0..trail {
                            let y = head - step as f32 * cell;
                            if y < -cell || y > height {
                                continue;
                            }
                            let fade = (1.0 - step as f32 / trail as f32).powf(1.6);
                            let x = column as f32 * pitch;
                            if step < 3 {
                                let glyph =
                                    GLYPHS[((u * 160.0) as u64 + column * 5 + step as u64) as usize
                                        % GLYPHS.len()] as char;
                                let color = if step == 0 {
                                    Color::rgb(220, 255, 235)
                                } else {
                                    GREEN
                                };
                                canvas.text(
                                    x,
                                    y,
                                    pitch,
                                    &glyph.to_string(),
                                    12.0,
                                    700,
                                    alpha(color, (fade * 255.0) as u8),
                                    TextAlign::Center,
                                );
                            } else {
                                canvas.rect(
                                    x + pitch * 0.3,
                                    y + 2.0,
                                    pitch * 0.4,
                                    cell - 6.0,
                                    alpha(GREEN, (fade * 120.0) as u8),
                                );
                            }
                        }
                    }
                    canvas.pop_clip();
                },
            );
        },
    );
}

/// A log that scrolls as events arrive, with a blinking cursor.
#[composable]
pub fn terminal_panel() {
    const EVENTS: [(&str, Color); 9] = [
        ("ICE breached on node 4f:2a", PINK),
        ("handshake accepted 0xA7C3", CYAN),
        ("packet loss 0.3% on relay-12", YELLOW),
        ("daemon spawned: ghost.exe", GREEN),
        ("firewall rule table updated", CYAN),
        ("TRACE DETECTED, rerouting", RED),
        ("cache flushed 2.4GB", GREEN),
        ("neural link stable 98%", CYAN),
        ("credits transferred 12,400", YELLOW),
    ];
    let count = remember(|| 0_u64);
    let lines = remember(|| {
        (0..8_u64)
            .map(|n| (n, (n as usize * 4 + (noise(n * 9) * 3.0) as usize) % 9))
            .collect::<Vec<_>>()
    });
    {
        let (count, lines) = (count.clone(), lines.clone());
        launched_effect((), move |scope| async move {
            loop {
                scope.delay(Duration::from_millis(700)).await;
                let n = count.get() + 8;
                count.set(n + 1);
                lines.update(|lines| {
                    lines.remove(0);
                    lines.push((n, (n as usize * 4 + (noise(n * 13) * 3.0) as usize) % 9));
                });
            }
        });
    }
    let blink = clock(1.0).get() < 0.5;
    panel(
        Modifier::new().weight(1.0),
        YELLOW,
        "TTY//11",
        "Deck terminal",
        "ssh root@nightcity",
        || {
            for (id, event) in lines.get() {
                key(id, || {
                    let (message, color) = EVENTS[event];
                    row_arranged(
                        Modifier::new().padding_xy(0.0, 2.0),
                        Arrangement::SpacedBy(8.0),
                        Alignment::Start,
                        || {
                            mono(Modifier::new(), &format!("[{id:04}]"), 11.0, 400, DIM);
                            mono(Modifier::new(), message, 11.0, 500, color);
                        },
                    );
                });
            }
            mono(
                Modifier::new().padding_xy(0.0, 2.0),
                if blink { "> █" } else { ">" },
                11.0,
                700,
                YELLOW,
            );
        },
    );
}

/// A lissajous trace and a scrolling wave on a graticule, both glowing.
#[composable]
pub fn scope_panel() {
    let u = clock(10.0).get();
    panel(
        Modifier::new().weight(1.0),
        PINK,
        "OSC//12",
        "Oscilloscope",
        "Phase-locked on channel 2",
        || {
            canvas(
                Modifier::new().fill_max_width().height(190.0),
                move |canvas| {
                    let (w, h) = (canvas.width(), canvas.height());
                    for i in 0..=8 {
                        let x = w * i as f32 / 8.0;
                        canvas.rect(x, 0.0, 1.0, h, alpha(CYAN, if i == 4 { 50 } else { 20 }));
                    }
                    for i in 0..=5 {
                        let y = h * i as f32 / 5.0;
                        canvas.rect(0.0, y, w, 1.0, alpha(CYAN, if i == 2 { 50 } else { 20 }));
                    }
                    let phase = turns(u, 1.0);
                    let lissajous: Vec<(f32, f32)> = (0..=140)
                        .map(|i| {
                            let a = i as f32 / 140.0 * TAU;
                            (
                                w / 2.0 + (a * 3.0 + phase).sin() * w * 0.4,
                                h / 2.0 + (a * 2.0).sin() * h * 0.4,
                            )
                        })
                        .collect();
                    neon_line(canvas, &lissajous, 1.4, PINK);
                    let wave: Vec<(f32, f32)> = (0..=60)
                        .map(|i| {
                            let f = i as f32 / 60.0;
                            let carrier = (f * PI * 6.0 - turns(u, 3.0)).sin();
                            let envelope = (f * PI).sin();
                            (f * w, h * 0.82 + carrier * envelope * h * 0.1)
                        })
                        .collect();
                    neon_line(canvas, &wave, 1.0, CYAN);
                },
            );
        },
    );
}

/// A strip of headlines that slides past without a seam.
#[composable]
pub fn ticker() {
    const ITEMS: [(&str, Color); 6] = [
        ("// BREACH CONTAINED IN SECTOR 7", GREEN),
        ("ARASAKA STOCK +4.2%", CYAN),
        ("// CURFEW IN EFFECT 02:00", YELLOW),
        ("NEW ICE PATCH 9.3 DEPLOYED", PINK),
        ("// MAX-TAC DISPATCHED TO WATSON", RED),
        ("RIPPERDOC OPEN 24H", CYAN),
    ];
    let u = clock(36.0).get();
    canvas(
        Modifier::new().fill_max_width().height(34.0),
        move |canvas| {
            let (w, h) = (canvas.width(), canvas.height());
            canvas.rounded_rect(0.0, 0.0, w, h, 3.0, Color::rgba(11, 6, 26, 220));
            canvas.stroke_rect(0.0, 0.0, w, h, 3.0, 1.0, alpha(PINK, 90));
            canvas.push_clip(0.0, 0.0, w, h);
            let pitch = 330.0;
            let span = pitch * ITEMS.len() as f32;
            for (index, (message, color)) in ITEMS.iter().enumerate() {
                let x = (index as f32 * pitch - u * span).rem_euclid(span) - pitch;
                canvas.text(
                    x,
                    9.0,
                    pitch - 20.0,
                    message,
                    12.0,
                    600,
                    *color,
                    TextAlign::Left,
                );
            }
            canvas.pop_clip();
        },
    );
}
