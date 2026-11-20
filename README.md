# Rustle

[![CI](https://github.com/JoakimCarlsson/rustle/actions/workflows/ci.yml/badge.svg)](https://github.com/JoakimCarlsson/rustle/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024_edition-orange?logo=rust)](https://www.rust-lang.org/)

A declarative UI framework for Rust, modeled on Jetpack Compose. UI is written as composable functions, state is remembered across recompositions, layout is driven by modifiers, and rendering is a swappable backend, with `wgpu` first.

## Features

- **Runtime**: `remember`, snapshot state, derived state, effects and tasks, composition locals, skipping and scoped recomposition
- **Layout**: `Modifier` chains, rows, columns, stacks, flow and constraint layouts, lazy lists and grids
- **Drawing**: gradients, shadows, clipping, layer effects, backdrop blur, vector paths, SVG and custom WGSL shaders
- **Text**: rich, wrapped text, selection, text fields with input methods and undo
- **Input**: Compose-style pointer events, gestures, drag and drop, focus, keyboard, touch and pen
- **Widgets**: a Material-style theme and a full set of components: buttons, dialogs, popups, menus, tooltips, sliders, tabs, charts and more
- **Animation**: `animate_*_as_state`, `Animatable`, transitions, springs, keyframes and path morphing
- **Windows**: multiple windows, window state, placement and focus
- **Accessibility**: a semantics tree sent to the platform screen reader through AccessKit
- **Embedding**: `rustle::Embedded` runs an interface inside a host that owns the window, the GPU device and the frame
- **Icons**: every [Phosphor](https://phosphoricons.com) icon as a compile-time constant

## Quick start

```toml
[dependencies]
rustle = { git = "https://github.com/JoakimCarlsson/rustle" }
```

```rust
use rustle::prelude::*;

#[composable]
fn counter() {
    let count = remember(|| 0);

    column_with(Modifier::new().padding(24.0), || {
        text(format!("Count: {}", count.get()));
        button("Increment", move || count.update(|n| *n += 1));
    });
}

fn main() {
    rustle::run(counter);
}
```

## Examples

```bash
cargo run --example hello_world   # full dashboard showcase (or `make run`)
cargo run --example multi_window
cargo run --example text
cargo run --example graphics
cargo run --example theme
cargo run --example embedded -p rustle   # headless embedding check
```

`hello_world` is a complete dashboard: navigation, a command palette, tables, charts, forms, a component gallery and a motion page.

## Crates

| Crate | Purpose |
|-------|---------|
| `rustle` | Public API, prelude, event loop, windows, embedding |
| `rustle-runtime` | Composition, state, effects, snapshots |
| `rustle-ui` | Nodes, `Modifier`, layout, input, semantics |
| `rustle-foundation` | Basic composables: text, rows, columns, lazy lists, text fields |
| `rustle-widgets` | Theme and Material-style components |
| `rustle-animation` | Animation specs, transitions, `Animatable` |
| `rustle-charts` | Area, line, bar and donut charts, sparklines, QR codes |
| `rustle-navigation` | Back stack and nav host |
| `rustle-text` | Text styles and layout |
| `rustle-graphics` | Colors, images, SVG, paths, draw commands |
| `rustle-geometry` / `rustle-unit` | Geometry and units |
| `rustle-wgpu` | The wgpu renderer |
| `rustle-shaders` | WGSL shaders |
| `rustle-icons` | Phosphor icons |
| `rustle-macros` | `#[composable]` |

See [docs/architecture.md](docs/architecture.md) for what belongs in each crate, the frame flow and current limitations.

## Embedding

A host that owns the window, the GPU device and the frame can run a Rustle interface with `rustle::Embedded` and draw it into its own texture through `rustle_wgpu::Renderer::with_device`. The host forwards size, pointer, keyboard and input-method events, and learns from each whether the interface used it.

## Snapshots

Draw one frame offscreen to a PNG, without a window:

```bash
RUSTLE_SNAPSHOT=out.png RUSTLE_SNAPSHOT_SIZE=800x600 cargo run --example hello_world
```

| Variable | Effect |
|----------|--------|
| `RUSTLE_SNAPSHOT_LIGHT=1` | Light theme |
| `RUSTLE_SNAPSHOT_SCALE=2` | Scale factor, as on a HiDPI screen |
| `RUSTLE_SNAPSHOT_INPUT="click:150,237;type:Ada;key:Backspace"` | Replay input before the picture |
| `RUSTLE_SNAPSHOT_STEP_MS=0` | Take the picture right after the last step |
| `RUSTLE_DEBUG_INPUT=1` | Print the input a real window receives |

Input steps: `click:x,y`, `move:x,y`, `type:text`, `key:Name` and `wait:ms`, separated by `;`. Works for any application started through `rustle::run`.

## Benchmarks

```bash
BENCH_ROWS=600 BENCH_FRAMES=300 cargo run --release -p rustle --example bench
```

Reports composition, layout/paint and render-plus-readback medians, and total frame median/p95/p99. `BENCH_SCENARIO=effects` or `list` runs one scenario.

## Development

```bash
make fmt
make lint
cargo test --workspace
```

## License

MIT. See [LICENSE](LICENSE). Phosphor icons are MIT, see [LICENSE-PHOSPHOR](crates/rustle-icons/LICENSE-PHOSPHOR).
