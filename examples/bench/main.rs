//! Measures what one animated frame costs for a tree of a few thousand nodes.
//!
//! Run it with `cargo run --release --example bench`. It composes a long list
//! next to a value that changes every frame, and times the three stages of a
//! frame separately: composing, laying out and painting into commands, and
//! turning the commands into GPU work (`layers`, measured through an offscreen
//! snapshot, which also reads the picture back, so it is an upper bound).
//!
//! Set `BENCH_ROWS` to change the number of rows (default 600, five nodes each
//! plus a few more) and `BENCH_FRAMES` for the number of timed frames.

use std::time::{Duration, Instant};

use rustle::prelude::*;
use rustle_geometry::Size;
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

/// Builds the scene. With `skip`, each row is a skippable block, so rows whose
/// inputs did not change are not composed again.
fn scene(rows: usize, skip: bool, value: f32) {
    column_with(Modifier::new().fill_max_size(), || {
        // The part that changes every frame.
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
        let state = remember_scroll_state();
        column_with(
            Modifier::new()
                .weight(1.0)
                .fill_max_width()
                .vertical_scroll(&state),
            || {
                for index in 0..rows {
                    key(index, || {
                        if skip {
                            rustle::skippable(index, || row_item(index));
                        } else {
                            row_item(index);
                        }
                    });
                }
            },
        );
    });
}

/// The time of each timed frame, per stage.
#[derive(Default)]
struct Timings {
    compose: Vec<Duration>,
    layout: Vec<Duration>,
    gpu: Vec<Duration>,
}

/// The median in milliseconds, which other work on the machine disturbs less than the mean.
fn median(times: &mut [Duration]) -> f64 {
    times.sort();
    times[times.len() / 2].as_secs_f64() * 1000.0
}

fn run(
    rows: usize,
    skip: bool,
    frames: u32,
) -> Result<(Timings, usize), Box<dyn std::error::Error>> {
    let (width, height) = (900, 700);
    let shared = std::rc::Rc::new(std::cell::RefCell::new(None::<State<f32>>));
    let inner = std::rc::Rc::clone(&shared);
    let mut composition = rustle_ui::composition(move || {
        let value = remember(|| 0.0_f32);
        *inner.borrow_mut() = Some(value.clone());
        scene(rows, skip, value.get());
    });
    composition.compose();
    let value = shared.borrow().clone().ok_or("no state")?;

    let mut renderer = Renderer::offscreen(width, height)?;
    let constraints = Constraints::loose(Size::new(width as f32, height as f32));
    let mut timings = Timings::default();
    let mut nodes = 0;
    // Warm up: shapes the text, fills caches.
    for frame in 0..frames + 5 {
        value.set((frame % 100) as f32 / 100.0);
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
        if frame >= 5 {
            timings.compose.push(composed);
            timings.layout.push(laid_out);
            timings.gpu.push(drawn);
        }
    }
    Ok((timings, nodes))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = std::env::var("BENCH_ROWS")
        .ok()
        .and_then(|rows| rows.parse().ok())
        .unwrap_or(600);
    let frames: u32 = std::env::var("BENCH_FRAMES")
        .ok()
        .and_then(|frames| frames.parse().ok())
        .unwrap_or(60);
    for skip in [false, true] {
        let (mut timings, commands) = run(rows, skip, frames)?;
        println!(
            "rows={rows} skippable={skip} commands={commands}: median per frame: compose {:.2} ms, layout+paint {:.2} ms, gpu {:.2} ms",
            median(&mut timings.compose),
            median(&mut timings.layout),
            median(&mut timings.gpu),
        );
    }
    Ok(())
}
