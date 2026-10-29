//! The neon palette and the small drawing tricks every panel shares.

use std::f32::consts::TAU;
use std::time::Duration;

use rustle::prelude::*;

pub const NIGHT: Color = Color::rgb(8, 4, 20);
pub const PINK: Color = Color::rgb(255, 43, 214);
pub const CYAN: Color = Color::rgb(0, 240, 255);
pub const YELLOW: Color = Color::rgb(252, 238, 10);
pub const PURPLE: Color = Color::rgb(140, 60, 255);
pub const RED: Color = Color::rgb(255, 56, 80);
pub const GREEN: Color = Color::rgb(57, 255, 136);
pub const INK: Color = Color::rgb(226, 236, 255);
pub const DIM: Color = Color::rgb(128, 116, 184);

/// `color` at the given opacity, from 0 to 255.
pub fn alpha(color: Color, a: u8) -> Color {
    Color { a, ..color }
}

/// A blend from `a` to `b`, with `t` from 0 to 1.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let channel =
        |from: u8, to: u8| (f32::from(from) + (f32::from(to) - f32::from(from)) * t) as u8;
    Color::rgba(
        channel(a.r, b.r),
        channel(a.g, b.g),
        channel(a.b, b.b),
        channel(a.a, b.a),
    )
}

/// A heat ramp: near black, purple, hot pink, yellow.
pub fn ramp(t: f32) -> Color {
    let stops = [Color::rgb(12, 6, 36), PURPLE, PINK, YELLOW];
    let scaled = t.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
    let index = (scaled as usize).min(stops.len() - 2);
    mix(stops[index], stops[index + 1], scaled - index as f32)
}

/// A value that runs from 0 to 1 over `seconds`, again and again. Animations
/// that turn a whole number of times per run loop without a seam.
#[composable]
pub fn clock(seconds: f32) -> State<f32> {
    infinite_float_repeating(0.0, 1.0, Duration::from_secs_f32(seconds), Easing::Linear)
}

/// `u` (0 to 1) as an angle that turns `count` times.
pub fn turns(u: f32, count: f32) -> f32 {
    u * TAU * count
}

/// A small pseudo-random number from `seed`, from 0 to 1.
pub fn noise(seed: u64) -> f32 {
    let mixed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mixed = (mixed ^ (mixed >> 29)).wrapping_mul(0xbf58476d1ce4e5b9);
    ((mixed >> 40) as f32) / (1u64 << 24) as f32
}

/// A series that drifts around `base`, so charts have something to show.
pub fn wander(count: usize, seed: f32, base: f32, amplitude: f32) -> Vec<f32> {
    (0..count)
        .map(|i| {
            let t = i as f32;
            base + amplitude
                * (0.55 * (t * 0.31 + seed).sin()
                    + 0.3 * (t * 0.11 + seed * 2.0).sin()
                    + 0.15 * (t * 0.9 + seed * 3.0).sin())
        })
        .collect()
}

/// A soft glow around (`x`, `y`) that fades out over `radius`; `strength` is
/// the opacity at its core.
pub fn glow(canvas: &mut Canvas, x: f32, y: f32, radius: f32, color: Color, strength: u8) {
    let core = radius * 0.2;
    canvas.shadow(
        x - core,
        y - core,
        core * 2.0,
        core * 2.0,
        core,
        radius,
        alpha(color, strength),
    );
}

/// A line that burns: a wide dim halo, a brighter band, and a white-hot core.
pub fn neon_line(canvas: &mut Canvas, points: &[(f32, f32)], width: f32, color: Color) {
    canvas.polyline(points, width * 5.0, alpha(color, 26));
    canvas.polyline(points, width * 2.2, alpha(color, 90));
    canvas.polyline(points, width, mix(color, Color::rgb(255, 255, 255), 0.55));
}

/// One line of text in a chosen size, weight and color.
#[composable]
pub fn label(modifier: Modifier, content: &str, size: f32, weight: u16, color: Color) {
    let style = TextStyle {
        color,
        font_size: size,
        line_height: size * 1.3,
        font_weight: FontWeight(weight),
        ..TextStyle::default()
    };
    text_styled(modifier, content, style);
}

/// Like [`label`], set in the monospace face.
#[composable]
pub fn mono(modifier: Modifier, content: &str, size: f32, weight: u16, color: Color) {
    let style = TextStyle {
        color,
        font_size: size,
        line_height: size * 1.35,
        font_weight: FontWeight(weight),
        font_family: FontFamily::Monospace,
        ..TextStyle::default()
    };
    text_styled(modifier, content, style);
}
