//! Canvases that redraw every frame: a heat field, a net trace and a spectrum.
//! Each turns a whole number of times per loop, so none has a seam.

use rustle::prelude::*;

use crate::fx::*;
use crate::hud::panel;

/// A grid of cells whose color follows overlapping waves, the way a fragment shader would.
#[composable]
pub fn heat_field_panel() {
    let u = clock(12.0).get();
    panel(
        Modifier::new().weight(1.0),
        PURPLE,
        "GRID//04",
        "Neural grid",
        "Four interfering waves and a scan row",
        || {
            canvas(
                Modifier::new().fill_max_width().height(190.0),
                move |canvas| {
                    let (columns, rows) = (30, 12);
                    let gap = 2.0;
                    let cell_w = (canvas.width() - gap * (columns - 1) as f32) / columns as f32;
                    let cell_h = (canvas.height() - gap * (rows - 1) as f32) / rows as f32;
                    let (cx, cy) = (columns as f32 * 0.5, rows as f32 * 0.5);
                    let scan = (u * rows as f32 * 2.0) % rows as f32;
                    for row in 0..rows {
                        for column in 0..columns {
                            let (x, y) = (column as f32, row as f32);
                            let reach = ((x - cx).powi(2) + (y - cy).powi(2) * 2.0).sqrt();
                            let wave = (x * 0.42 + turns(u, 1.0)).sin()
                                + (y * 0.8 - turns(u, 2.0)).sin()
                                + ((x + y) * 0.3 + turns(u, 1.0)).sin()
                                + (reach * 0.55 - turns(u, 3.0)).sin();
                            let near_scan = (1.0 - (y - scan).abs().min(3.0) / 3.0).max(0.0);
                            let level = (wave / 8.0 + 0.5 + near_scan * 0.25).clamp(0.0, 1.0);
                            let (px, py) =
                                (column as f32 * (cell_w + gap), row as f32 * (cell_h + gap));
                            let hot = level * level * 1.5;
                            if hot > 0.85 {
                                glow(canvas, px + cell_w / 2.0, py + cell_h / 2.0, 14.0, PINK, 60);
                            }
                            canvas.rounded_rect(px, py, cell_w, cell_h, 2.0, ramp(hot));
                        }
                    }
                },
            );
        },
    );
}

/// Drifting nodes joined by lines when near each other, and drawn toward the pointer.
#[composable]
pub fn constellation_panel() {
    let u = clock(30.0).get();
    let pointer = remember(|| None::<(f32, f32)>);
    let moved = pointer.clone();
    panel(
        Modifier::new().weight(1.0),
        CYAN,
        "TRACE//05",
        "Net trace",
        "Nodes drift on their own; sweep the pointer through them",
        || {
            canvas(
                Modifier::new()
                    .fill_max_width()
                    .height(250.0)
                    .on_pointer_move(move |at| moved.set(at.map(|at| (at.x, at.y)))),
                move |canvas| {
                    let (width, height) = (canvas.width(), canvas.height());
                    let nodes: Vec<(f32, f32)> = (0..42_u64)
                        .map(|i| {
                            let kx = 1.0 + (noise(i * 5) * 3.0).floor();
                            let ky = 1.0 + (noise(i * 11) * 3.0).floor();
                            let (px, py) = (noise(i * 17) * 6.28, noise(i * 23) * 6.28);
                            (
                                width * (0.5 + 0.46 * (turns(u, kx) + px).sin()),
                                height * (0.5 + 0.44 * (turns(u, ky) + py).cos()),
                            )
                        })
                        .collect();
                    let reach = 84.0;
                    for (a, from) in nodes.iter().enumerate() {
                        for to in &nodes[a + 1..] {
                            let gap = ((from.0 - to.0).powi(2) + (from.1 - to.1).powi(2)).sqrt();
                            if gap < reach {
                                let strength = 1.0 - gap / reach;
                                canvas.line(*from, *to, 1.0, alpha(CYAN, (strength * 170.0) as u8));
                            }
                        }
                    }
                    let cursor = pointer.get();
                    for (index, node) in nodes.iter().enumerate() {
                        let near = cursor.map_or(0.0, |c| {
                            let gap = ((node.0 - c.0).powi(2) + (node.1 - c.1).powi(2)).sqrt();
                            (1.0 - gap / 120.0).max(0.0)
                        });
                        if let (Some(c), true) = (cursor, near > 0.0) {
                            neon_line(canvas, &[*node, c], 1.0, PINK);
                        }
                        let tint = mix(CYAN, PINK, near);
                        let size = 2.0 + noise(index as u64 * 3) * 2.0 + near * 3.0;
                        glow(canvas, node.0, node.1, size * 6.0, tint, 90);
                        canvas.circle(node.0, node.1, size, tint);
                    }
                    if let Some(c) = cursor {
                        glow(canvas, c.0, c.1, 60.0, PINK, 110);
                        canvas.stroke_circle(c.0, c.1, 14.0, 1.5, PINK);
                    }
                },
            );
        },
    );
}

/// A mirrored equalizer with a soft floor reflection.
#[composable]
pub fn spectrum_panel() {
    let u = clock(8.0).get();
    panel(
        Modifier::new().weight(1.0),
        PINK,
        "AUD//07",
        "Spectrum",
        "Bands mirrored around the horizon line",
        || {
            canvas(
                Modifier::new().fill_max_width().height(250.0),
                move |canvas| {
                    let bands = 52;
                    let gap = 3.0;
                    let width = (canvas.width() - gap * (bands - 1) as f32) / bands as f32;
                    let middle = canvas.height() * 0.58;
                    canvas.shadow(
                        0.0,
                        middle - 40.0,
                        canvas.width(),
                        80.0,
                        30.0,
                        60.0,
                        alpha(PINK, 40),
                    );
                    for band in 0..bands {
                        let f = band as f32 / bands as f32;
                        let level = (0.5
                            + 0.5
                                * (turns(u, 2.0) + f * 9.0).sin()
                                * (turns(u, 1.0) - f * 5.0).cos())
                            * (1.0 - f * 0.5)
                            + 0.08;
                        let half = (level * (middle - 8.0)).max(3.0);
                        let x = band as f32 * (width + gap);
                        let color = mix(PINK, CYAN, f);
                        canvas.gradient(
                            x,
                            middle - half,
                            width,
                            half,
                            1.5,
                            color,
                            alpha(color, 70),
                            90.0,
                        );
                        canvas.gradient(
                            x,
                            middle + 2.0,
                            width,
                            half * 0.55,
                            1.5,
                            alpha(color, 80),
                            alpha(color, 0),
                            90.0,
                        );
                        canvas.rect(x, middle - half - 5.0, width, 2.0, alpha(INK, 210));
                    }
                    canvas.rect(0.0, middle, canvas.width(), 1.0, alpha(INK, 90));
                },
            );
        },
    );
}
