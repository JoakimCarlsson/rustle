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
