//! Interface benchmarks without a GPU: composing a list of rows next to a value that changes
//! every frame, measuring and painting it, painting it again unchanged, and scrolling lazy
//! lists whose rows are all as tall or each as tall as its text.
//!
//! ```sh
//! CARGO_TARGET_DIR=build cargo bench -p rustle-bench --bench interface
//! ```

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rustle::prelude::*;
use rustle::{Constraints, Embedded, LazyContent, Offset, Size, State, uniform_list};
use rustle_bench::{Stats, report, selected, words};
use rustle_ui::{DisplayOptions, LaidOut, PointerState};
use rustle_wgpu::TextMetrics;

/// How many rows the composed list holds.
const ROWS: usize = 600;

/// How many frames each benchmark is measured over.
const FRAMES: usize = 200;

/// How many items each lazy list holds.
const ITEMS: usize = 10_000;

/// The window the interface is laid out in, in logical pixels.
const WINDOW: (u32, u32) = (900, 700);

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

/// A bar as wide as `value` says, over a list of [`ROWS`] rows.
fn scene(value: &State<f32>) {
    column_with(Modifier::new().fill_max_size(), || {
        row_with(Modifier::new().fill_max_width().height(24.0), || {
            spacer(
                Modifier::new()
                    .width(10.0 + value.get() * 400.0)
                    .fill_max_height()
                    .background_rounded(Color::rgb(80, 160, 255), 4.0),
            );
            text(format!("{:.0}%", value.get() * 100.0));
        });
        column(|| {
            for index in 0..ROWS {
                key(index, || row_item(index));
            }
        });
    });
}

/// Prints the times in `samples` under `name`.
fn print(name: &str, mut samples: Vec<Duration>) {
    samples.sort();
    report(&Stats {
        name: name.to_owned(),
        samples,
    });
}

/// Composes, measures and paints the list of rows once a frame, the bar changing every
/// frame, timing each stage, then paints the measured tree again unchanged.
fn compose_layout_paint() {
    if !["interface/compose", "interface/layout", "interface/paint"]
        .iter()
        .any(|name| selected(name))
    {
        return;
    }
    let shared: Rc<RefCell<Option<State<f32>>>> = Rc::default();
    let inner = Rc::clone(&shared);
    let mut composition = rustle_ui::composition(move || {
        let value = remember(|| 0.0_f32);
        *inner.borrow_mut() = Some(value.clone());
        scene(&value);
    });
    composition.compose();
    let value = shared
        .borrow()
        .clone()
        .expect("the scene remembers its value");
    let mut metrics = TextMetrics::new();
    let constraints = Constraints::loose(Size::new(WINDOW.0 as f32, WINDOW.1 as f32));
    let (mut compose, mut layout, mut paint, mut repaint) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for frame in 0..FRAMES + 3 {
        value.set((frame % 100) as f32 / 100.0);
        let started = Instant::now();
        composition.recompose();
        let composed = started.elapsed();
        let tree = composition.output().expect("the scene composed").clone();
        let started = Instant::now();
        let laid = LaidOut::new(&tree, constraints, &mut metrics, DisplayOptions::default());
        let measured = started.elapsed();
        let started = Instant::now();
        let list = laid.paint(PointerState::default());
        let painted = started.elapsed();
        let started = Instant::now();
        std::hint::black_box(laid.paint_reusing(PointerState::default(), list));
        let painted_again = started.elapsed();
        if frame >= 3 {
            compose.push(composed);
            layout.push(measured);
            paint.push(painted);
            repaint.push(painted_again);
        }
    }
    print("interface/compose_600_rows", compose);
    print("interface/layout_600_rows", layout);
    print("interface/paint_600_rows", paint);
    print("interface/paint_reusing_600_rows", repaint);
}

/// Scrolls an interface made of `root` by `step` logical pixels a frame for [`FRAMES`]
/// frames, timing each update, and prints the times under `name`.
fn scroll(name: &str, step: f32, root: impl FnMut() + 'static) {
    if !selected(name) {
        return;
    }
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize(WINDOW, 1.0);
    embedded.update();
    embedded.pointer_moved(Offset::new(200.0, 200.0));
    embedded.update();
    let before = texts(&embedded);
    let mut samples = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        let started = Instant::now();
        let took = embedded.wheel(Offset::new(0.0, -step));
        embedded.update();
        assert!(took, "{name}: the list took the wheel");
        samples.push(started.elapsed());
    }
    assert_ne!(texts(&embedded), before, "{name}: the list scrolled");
    print(name, samples);
}

/// The plain texts `embedded` draws, in order.
fn texts(embedded: &Embedded) -> Vec<String> {
    embedded
        .commands()
        .iter()
        .filter_map(|command| match command {
            rustle_graphics::DrawCommand::Text { content, .. } => Some(content.to_string()),
            _ => None,
        })
        .collect()
}

/// The text of variable item `index`: a sentence of a length that varies by item, so the
/// items wrap to different heights.
fn sentence(index: usize) -> String {
    words(4 + index * 7 % 40, index as u64).join(" ")
}

/// Runs every interface benchmark.
fn main() {
    compose_layout_paint();
    scroll("interface/lazy_list_variable_scroll", 37.0, || {
        let state = remember_lazy_list_state();
        lazy_list(Modifier::new().fill_max_size(), &state, ITEMS, |index| {
            column_with(Modifier::new().fill_max_width().padding(6.0), || {
                text(format!("Message {index}"));
                text(sentence(index));
            });
        });
    });
    scroll("interface/uniform_list_scroll", 37.0, || {
        let state = remember_lazy_list_state();
        uniform_list(
            Modifier::new().fill_max_size(),
            &state,
            LazyContent::default(),
            ITEMS,
            24.0,
            |index| {
                row_with(Modifier::new().fill_max_width().height(24.0), || {
                    text(format!("Row {index}"));
                    spacer(Modifier::new().weight(1.0));
                    text(format!("{}", index * 13 % 1000));
                });
            },
        );
    });
}
