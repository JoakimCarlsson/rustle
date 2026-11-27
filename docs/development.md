# Development

## Checks

```sh
make fmt
make lint
cargo test --workspace
```

## Examples

```sh
cargo run --example hello_world          # full dashboard showcase (or `make run`)
cargo run --example multi_window
cargo run --example text
cargo run --example graphics
cargo run --example theme
cargo run --example embedded -p rustle   # headless embedding check
```

`hello_world` is a complete dashboard: navigation, a command palette, tables,
charts, forms, a component gallery and a motion page.

## Snapshots

Draw one frame offscreen to a PNG, without a window:

```sh
RUSTLE_SNAPSHOT=out.png RUSTLE_SNAPSHOT_SIZE=800x600 cargo run --example hello_world
```

| Variable | Effect |
|----------|--------|
| `RUSTLE_SNAPSHOT_LIGHT=1` | Light theme |
| `RUSTLE_SNAPSHOT_SCALE=2` | Scale factor, as on a HiDPI screen |
| `RUSTLE_SNAPSHOT_INPUT="click:150,237;type:Ada;key:Backspace"` | Replay input before the picture |
| `RUSTLE_SNAPSHOT_STEP_MS=0` | Take the picture right after the last step |
| `RUSTLE_DEBUG_INPUT=1` | Print the input a real window receives |

Input steps are `click:x,y`, `move:x,y`, `type:text`, `key:Name` and
`wait:ms`, separated by `;`. This works for any application started through
`rustle::run`.

## Benchmarks

```sh
BENCH_ROWS=600 BENCH_FRAMES=300 cargo run --release -p rustle --example bench
```

This reports composition, layout/paint and render-plus-readback medians, and
total frame median, p95 and p99. `BENCH_SCENARIO=effects` or `list` runs one
scenario.

The `rustle-bench` crate holds benchmarks of the separate stages, run with a
small harness of its own on the standard library alone:

```sh
make bench
CARGO_TARGET_DIR=build cargo bench -p rustle-bench
CARGO_TARGET_DIR=build cargo bench -p rustle-bench --bench text -- streamed
```

Words after `--` keep only the benchmarks whose names hold one of them. Each
benchmark prints its median, 95th percentile, slowest and mean run in
microseconds, the total and how many runs were measured.

| Bench | What it measures |
| --- | --- |
| `text` | Measuring and laying out text without a GPU: a short label, a long paragraph in one style and with markdown's bold words and links, a paragraph with a run per word, a paragraph streamed in three words a frame with its newest characters fading in, and such a paragraph after six finished ones. |
| `interface` | Composing, measuring, painting and painting again unchanged a list of 600 rows beside a value that changes every frame, and scrolling lazy lists of 10 000 items, as tall as their text or all as tall. |
| `renderer` | Splitting a frame's draw commands into layers and shaping their text, without the GPU: the 600 rows, and a streamed paragraph. It needs a GPU adapter for the renderer, and skips itself without one. |

Text is shaped with cosmic-text from crates.io. Rustle keeps it cheap itself
(see `crates/rustle-wgpu/src/paragraphs.rs`): neighbouring runs set alike are
merged and the commonest style becomes the default, so cosmic-text, which
checks every word against every span of its paragraph, sees few spans;
paragraphs are kept shaped and laid out between texts; and a long paragraph
that grows is handed over as pieces cut where it last wrapped, so only its last
lines are shaped again.
