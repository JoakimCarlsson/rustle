# rustle

`rustle` is a declarative UI framework for Rust. An interface is written as
composable functions, state is remembered across recompositions, layout is
driven by modifiers, and rendering is a swappable backend, with `wgpu` first.

Modeled on [Jetpack Compose](https://developer.android.com/compose): the same
concepts, under the same names.

It is primarily used by
[pandemonium](https://github.com/JoakimCarlsson/pandemonium), which draws its
whole interface with it. Any other application can use it the same way, in a
window of its own or embedded in a host that owns the window.

## What it does

- **Runtime:** `remember`, snapshot state, derived state, effects and tasks,
  composition locals, skipping and scoped recomposition.
- **Layout:** `Modifier` chains, rows, columns, stacks, flow and constraint
  layouts, lazy lists and grids.
- **Drawing:** gradients, shadows, clipping, layer effects, backdrop blur,
  vector paths, SVG and custom WGSL shaders.
- **Text:** wrapped text, annotated strings with links, inline content and
  paragraph styles, selection, and text fields with input methods and undo.
- **Input:** pointer events, gestures, drag and drop, focus, keyboard, touch
  and pen.
- **Widgets:** a Material-style theme with buttons, dialogs, popups, menus,
  tooltips, sliders, tabs, charts and more, and every
  [Phosphor](https://phosphoricons.com) icon as a compile-time constant.
- **Animation:** `animate_*_as_state`, `Animatable`, transitions, springs,
  keyframes and path morphing.
- **Windows and embedding:** several windows with their own state, or
  `rustle::Embedded` inside a host that owns the window, the GPU device and
  the frame.
- **Accessibility:** a semantics tree sent to the platform screen reader
  through AccessKit.

## Install

```toml
[dependencies]
rustle = { git = "https://github.com/JoakimCarlsson/rustle" }
```

## Quick start

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

```sh
make run                       # the dashboard showcase
cargo run --example text       # one of the smaller examples
```

[Development](docs/development.md) lists every example and what it shows.

## Documentation

The full documentation is in [docs/](docs/README.md):

- [Architecture](docs/architecture.md): the crates, the frame flow, embedding
  and current limitations
- [Development](docs/development.md): examples, snapshots, benchmarks and
  checks

## License

MIT, see [LICENSE](LICENSE). Phosphor icons are MIT, see
[LICENSE-PHOSPHOR](crates/rustle-icons/LICENSE-PHOSPHOR).
