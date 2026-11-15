//! Canvases redrawn every frame from a looping clock.

use std::f32::consts::TAU;

use rustle::prelude::*;

use super::clock;
use crate::ui::panel;

#[composable]
pub(super) fn equalizer() {
    let theme = color_scheme();
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

pub(super) fn waves(t: f32) {
    let theme = color_scheme();
    let layers = [
        (theme.primary, 0.9, 1.0, 2.0),
        (theme.on_surface_variant, 0.6, 1.6, 1.5),
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
