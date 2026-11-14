//! Measures what one animated frame costs for a tree of a few thousand nodes.
//!
//! Run it with `cargo run --release --example bench`. It composes a long list
//! next to a value that changes every frame, and times the three stages of a
//! frame separately: composing, laying out and painting into commands, and
//! turning the commands into GPU work (measured through an offscreen snapshot,
//! including synchronization and readback, not isolated GPU execution). It also
//! reports the measured frame median, p95 and p99 to reveal long frames.
//!
//! Set `BENCH_ROWS` to change the number of rows (default 600, five nodes each
//! plus a few more) and `BENCH_FRAMES` for a positive number of timed frames.
//!
//! A second scenario, `effects`, draws a grid of cards that each have a rounded clip,
//! an effect layer and (every third card) a backdrop blur, straight to the renderer,
//! and times the frame including readback. Set `BENCH_SCENARIO` to `list` or `effects`
//! to run only one, `BENCH_CARDS` for the number of cards (default 300) and
//! `BENCH_SIZE` as `WIDTHxHEIGHT` (default 1920x1080).

use std::time::{Duration, Instant};

use rustle::prelude::*;
use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{DrawCommand, LayerEffect};
use rustle_unit::Constraints;
use rustle_wgpu::Renderer;

/// One row of the list: a background, a title, a subtitle and a trailing value.
fn row_item(index: usize) {
    row_with(
        Modifier::new()
            .fill_max_width()
            .height(40.0)
            .background_rounded(Color::rgb(30, 30, 36), 6.0)
            .padding(8.0),
        || {
            text(format!("Item {index}"));
            spacer(Modifier::new().weight(1.0));
            text("subtitle goes here");
            text(format!("{}", index * 7 % 100));
        },
    );
}

/// How the scene is composed.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// The root composes everything again on every change.
    Plain,
    /// Each row is a skippable block, so rows with equal inputs are not composed again.
    Skippable,
    /// The changing bar and the list are restartable blocks: the bar alone is composed
    /// again, and the list keeps its laid-out subtree.
    Restartable,
}

impl Mode {
    /// The name printed with the results.
    fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Skippable => "skippable",
            Self::Restartable => "restartable",
        }
    }
}

/// The bar and percentage that change every frame.
fn bar(value: f32) {
    row_with(
        Modifier::new().fill_max_width().height(24.0).padding(4.0),
        || {
            spacer(
                Modifier::new()
                    .width(10.0 + value * 400.0)
                    .fill_max_height()
                    .background_rounded(Color::rgb(80, 160, 255), 4.0),
            );
            text(format!("{:.0}%", value * 100.0));
        },
    );
}

/// The scrolling list of `rows` rows.
fn list(rows: usize, mode: Mode, state: &ScrollState) {
    column_with(
        Modifier::new()
            .weight(1.0)
            .fill_max_width()
            .vertical_scroll(state),
        || {
            for index in 0..rows {
                key(index, || {
                    if mode == Mode::Skippable {
                        rustle::skippable(index, || row_item(index));
                    } else {
                        row_item(index);
                    }
                });
            }
        },
    );
}

/// Builds the scene around the value that changes every frame.
fn scene(rows: usize, mode: Mode, value: &State<f32>) {
    let state = remember_scroll_state();
    column_with(Modifier::new().fill_max_size(), || {
        if mode == Mode::Restartable {
            let value = value.clone();
            rustle::restartable((), move || bar(value.get()));
            let state = state.clone();
            rustle::restartable(rows, move || list(rows, mode, &state));
        } else {
            bar(value.get());
            list(rows, mode, &state);
        }
    });
}

/// The time of each timed frame, per stage.
#[derive(Default)]
struct Timings {
    compose: Vec<Duration>,
    layout: Vec<Duration>,
    gpu: Vec<Duration>,
    total: Vec<Duration>,
}

/// The median in milliseconds, which other work on the machine disturbs less than the mean.
fn median(times: &mut [Duration]) -> f64 {
    times.sort();
    times[times.len() / 2].as_secs_f64() * 1000.0
}

/// Returns a nearest-rank percentile in milliseconds from ascending frame times.
///
/// # Panics
/// Panics if `times` is empty or `percent` is outside 1 through 100.
fn percentile(times: &[Duration], percent: usize) -> f64 {
    assert!(!times.is_empty() && (1..=100).contains(&percent));
    times[(times.len() * percent).div_ceil(100) - 1].as_secs_f64() * 1000.0
}

/// Times `frames` warmed-up animated frames for `rows` composed as `mode`.
///
/// # Errors
/// Returns an error if renderer creation, composition output or readback fails.
fn run(
    rows: usize,
    mode: Mode,
    frames: u32,
) -> Result<(Timings, usize), Box<dyn std::error::Error>> {
    let (width, height) = (900, 700);
    let shared = std::rc::Rc::new(std::cell::RefCell::new(None::<State<f32>>));
    let inner = std::rc::Rc::clone(&shared);
    let mut composition = rustle_ui::composition(move || {
        let value = remember(|| 0.0_f32);
        *inner.borrow_mut() = Some(value.clone());
        scene(rows, mode, &value);
    });
    composition.compose();
    let value = shared.borrow().clone().ok_or("no state")?;

    let mut renderer = Renderer::offscreen(width, height)?;
    let constraints = Constraints::loose(Size::new(width as f32, height as f32));
    let mut timings = Timings::default();
    let mut nodes = 0;
    for frame in 0..frames + 5 {
        value.set((frame % 100) as f32 / 100.0);
        let frame_start = Instant::now();
        let start = Instant::now();
        composition.recompose();
        let composed = start.elapsed();
        let tree = composition.output().ok_or("nothing composed")?;
        let start = Instant::now();
        let list = rustle_ui::display_list(tree, constraints, &mut renderer);
        let laid_out = start.elapsed();
        nodes = list.commands.len();
        let start = Instant::now();
        renderer.snapshot(&list.commands, 1.0)?;
        let drawn = start.elapsed();
        let total = frame_start.elapsed();
        if frame >= 5 {
            timings.compose.push(composed);
            timings.layout.push(laid_out);
            timings.gpu.push(drawn);
            timings.total.push(total);
        }
    }
    Ok((timings, nodes))
}

/// The commands for `cards` cards in a grid over a `width` by `height` window: each is a
/// rounded clip holding a fill, an effect layer and, for every third card, a backdrop blur.
fn effect_cards(cards: usize, width: f32, height: f32, shift: f32) -> Vec<DrawCommand> {
    let (card_width, card_height) = (120.0, 80.0);
    let columns = ((width / (card_width + 8.0)) as usize).max(1);
    let mut commands = vec![DrawCommand::Gradient {
        rect: Rect::new(Offset::new(0.0, 0.0), Size::new(width, height)),
        from: Color::rgb(30, 60, 160),
        to: Color::rgb(160, 40, 90),
        angle: 0.8,
        corner_radius: 0.0,
    }];
    for index in 0..cards {
        let x = (index % columns) as f32 * (card_width + 8.0) + 4.0 + shift;
        let y = (index / columns) as f32 * (card_height + 8.0) + 4.0;
        let rect = Rect::new(Offset::new(x, y), Size::new(card_width, card_height));
        commands.push(DrawCommand::PushRoundedClip {
            rect,
            radii: [12.0; 4],
        });
        if index % 3 == 0 {
            commands.push(DrawCommand::BackdropBlur {
                rect,
                corner_radius: 12.0,
                blur: 6.0,
            });
        }
        commands.push(DrawCommand::PushEffect {
            rect,
            effect: LayerEffect {
                alpha: 0.85,
                ..LayerEffect::default()
            },
        });
        commands.push(DrawCommand::Rect {
            rect,
            color: Color::rgba(255, 255, 255, 90),
            corner_radius: 0.0,
            border: None,
        });
        commands.push(DrawCommand::PopEffect);
        commands.push(DrawCommand::PopClip);
    }
    commands
}

/// Times `frames` warmed-up frames of the effect cards, returning each frame's time.
///
/// # Errors
/// Returns an error if the renderer cannot be created or a frame cannot be read back.
fn run_effects(
    cards: usize,
    (width, height): (u32, u32),
    frames: u32,
) -> Result<Vec<Duration>, Box<dyn std::error::Error>> {
    let mut renderer = Renderer::offscreen(width, height)?;
    let mut times = Vec::new();
    for frame in 0..frames + 5 {
        let commands = effect_cards(cards, width as f32, height as f32, (frame % 8) as f32);
        let start = Instant::now();
        renderer.snapshot(&commands, 1.0)?;
        if frame >= 5 {
            times.push(start.elapsed());
        }
    }
    Ok(times)
}

/// Runs the scenarios chosen by `BENCH_SCENARIO` and prints their timings.
///
/// # Errors
/// Returns an error if no frames are requested or a benchmark run fails.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scenario = std::env::var("BENCH_SCENARIO").unwrap_or_default();
    let frames: u32 = std::env::var("BENCH_FRAMES")
        .ok()
        .and_then(|frames| frames.parse().ok())
        .unwrap_or(60);
    if frames == 0 {
        return Err("BENCH_FRAMES must be greater than zero".into());
    }
    if scenario != "effects" {
        lists(frames)?;
    }
    if scenario != "list" {
        let cards = std::env::var("BENCH_CARDS")
            .ok()
            .and_then(|cards| cards.parse().ok())
            .unwrap_or(300);
        let size = std::env::var("BENCH_SIZE")
            .ok()
            .and_then(|size| {
                let (width, height) = size.split_once('x')?;
                Some((width.parse().ok()?, height.parse().ok()?))
            })
            .unwrap_or((1920, 1080));
        let mut times = run_effects(cards, size, frames)?;
        println!(
            "effects cards={cards} size={}x{}: render+readback median {:.2} ms, p95 {:.2} ms, p99 {:.2} ms",
            size.0,
            size.1,
            median(&mut times),
            percentile(&times, 95),
            percentile(&times, 99),
        );
    }
    Ok(())
}

/// Runs the list in each composition mode and prints the stage medians and frame-time percentiles.
///
/// # Errors
/// Returns an error if a benchmark run fails.
fn lists(frames: u32) -> Result<(), Box<dyn std::error::Error>> {
    let rows = std::env::var("BENCH_ROWS")
        .ok()
        .and_then(|rows| rows.parse().ok())
        .unwrap_or(600);
    for mode in [Mode::Plain, Mode::Skippable, Mode::Restartable] {
        let (mut timings, commands) = run(rows, mode, frames)?;
        println!(
            "rows={rows} mode={} commands={commands}: median per frame: compose {:.2} ms, layout+paint {:.2} ms, render+readback {:.2} ms; total median {:.2} ms, p95 {:.2} ms, p99 {:.2} ms",
            mode.name(),
            median(&mut timings.compose),
            median(&mut timings.layout),
            median(&mut timings.gpu),
            median(&mut timings.total),
            percentile(&timings.total, 95),
            percentile(&timings.total, 99),
        );
    }
    Ok(())
}

/// Regression coverage for frame-time percentile reporting.
#[cfg(test)]
mod tests {
    use super::*;

    /// Nearest-rank tails select samples without interpolation, including short runs.
    #[test]
    fn percentiles_report_tail_samples() {
        let times: Vec<_> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(percentile(&times, 95), 95.0);
        assert_eq!(percentile(&times, 99), 99.0);
        assert_eq!(percentile(&times[..1], 99), 1.0);
        assert_eq!(percentile(&times[..3], 95), 3.0);
    }
}
