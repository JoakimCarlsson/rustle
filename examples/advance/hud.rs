//! The neon-framed panels, stat tiles and status pills the scene is built from.

use std::time::Duration;

use rustle::prelude::*;

use crate::fx::*;

/// A dark panel in a neon frame with corner brackets, whose glow swells under the pointer.
#[composable]
pub fn panel(
    modifier: Modifier,
    accent: Color,
    tag: &'static str,
    heading: &str,
    caption: &str,
    content: impl FnOnce(),
) {
    let hovered = remember(|| false);
    let swell = animate_float_as_state(if hovered.get() { 1.0 } else { 0.0 }, tween(320)).get();
    let hover = hovered.clone();
    backdrop(
        modifier.on_hover(move |over| hover.set(over)),
        move |canvas| {
            let (w, h) = (canvas.width(), canvas.height());
            canvas.shadow(
                0.0,
                0.0,
                w,
                h,
                4.0,
                22.0 + swell * 22.0,
                alpha(accent, 20 + (swell * 56.0) as u8),
            );
            canvas.rounded_rect(0.0, 0.0, w, h, 4.0, Color::rgba(10, 5, 24, 250));
            canvas.gradient(
                0.0,
                0.0,
                w,
                90.0,
                4.0,
                alpha(accent, 22),
                alpha(accent, 0),
                90.0,
            );
            canvas.stroke_rect(
                0.0,
                0.0,
                w,
                h,
                4.0,
                1.0,
                alpha(accent, 60 + (swell * 120.0) as u8),
            );
            let arm = 18.0;
            let bright = alpha(accent, 255);
            for (x, y, dx, dy) in [
                (0.0, 0.0, 1.0, 1.0),
                (w, 0.0, -1.0, 1.0),
                (0.0, h, 1.0, -1.0),
                (w, h, -1.0, -1.0),
            ] {
                canvas.line((x, y), (x + dx * arm, y), 2.0, bright);
                canvas.line((x, y), (x, y + dy * arm), 2.0, bright);
            }
            canvas.text(
                w - 120.0,
                10.0,
                104.0,
                tag,
                10.0,
                600,
                alpha(accent, 170),
                TextAlign::Right,
            );
        },
        || {
            column_with(Modifier::new().fill_max_width().padding(22.0), || {
                if !heading.is_empty() {
                    mono(Modifier::new(), &heading.to_uppercase(), 13.0, 700, INK);
                    mono(Modifier::new(), caption, 11.0, 400, DIM);
                    spacer(Modifier::new().height(14.0));
                }
                content();
            });
        },
    );
}

/// A headline number that counts up from zero, with a change badge and a sparkline.
#[composable]
pub fn stat_tile(
    name: &'static str,
    target: f32,
    decimals: usize,
    suffix: &str,
    change: &str,
    accent: Color,
    seed: f32,
) {
    let ready = remember(|| false);
    let arm = ready.clone();
    launched_effect((), move |scope| async move {
        scope.delay(Duration::from_millis(300)).await;
        arm.set(true);
    });
    let shown = animate_float_as_state(if ready.get() { target } else { 0.0 }, tween(1800)).get();
    panel(Modifier::new().weight(1.0), accent, "", "", "", || {
        mono(Modifier::new(), &name.to_uppercase(), 11.0, 600, DIM);
        spacer(Modifier::new().height(6.0));
        mono(
            Modifier::new(),
            &format!("{shown:.decimals$}{suffix}"),
            30.0,
            700,
            INK,
        );
        spacer(Modifier::new().height(4.0));
        mono(
            Modifier::new(),
            change,
            11.0,
            600,
            if change.starts_with('▲') {
                GREEN
            } else {
                RED
            },
        );
        spacer(Modifier::new().height(8.0));
        sparkline(
            Modifier::new().fill_max_width().height(34.0),
            wander(28, seed, 50.0, 30.0),
            accent,
            2.0,
        );
    });
}

/// A pill with a pulsing dot, for status.
#[composable]
pub fn status_pill(text_on: &str, color: Color) {
    let pulse = clock(2.0).get();
    let beat = (turns(pulse, 1.0).sin() * 0.5 + 0.5).max(0.0);
    row_arranged(
        Modifier::new()
            .background_rounded(alpha(color, 26), 3.0)
            .border(1.0, alpha(color, 120), 3.0)
            .padding_xy(12.0, 6.0),
        Arrangement::Start,
        Alignment::Center,
        || {
            canvas(Modifier::new().width(16.0).height(16.0), move |canvas| {
                glow(canvas, 8.0, 8.0, 12.0, color, (60.0 + beat * 150.0) as u8);
                canvas.circle(8.0, 8.0, 3.0, color);
            });
            spacer(Modifier::new().width(6.0));
            mono(Modifier::new(), text_on, 11.0, 700, color);
        },
    );
}
