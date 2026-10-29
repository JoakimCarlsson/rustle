# Rustle

A declarative UI framework in Rust, inspired by Jetpack Compose. UI is described by composable functions; state is remembered across recompositions; layout is driven by modifiers; and rendering is a swappable backend, with `wgpu` first.

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

Run the showcase with:

```
cargo run --example hello_world
```

It is a full-window dashboard in a restrained, monochrome style, and everything on it works: the sidebar navigates eleven pages and collapses, Search opens a command palette, Quick Create adds a table row, the notifications and account menus act, and the dashboard's stat cards pick what the chart shows while hovering it reads out the point under the pointer. The table filters, sorts, pages, selects, hides columns and edits rows in a sheet; the overview has a working chat, calendar, role menus and invite dialog; the components page runs a transfer form against live balances, with confirmation. It follows the system's light or dark setting unless Settings overrides it. The code is split into modules under `examples/hello_world/`.

## What is here

- **Runtime:** state, `remember`, call-site identity with `key`, effects, tasks, composition locals, observable lists and maps, values saved across runs, skipping of unchanged blocks, and recomposition driven by the event loop.
- **Layout:** constraints, `Modifier` (padding, size, fill, weight, alignment, offset, aspect ratio, background, gradient, alpha, border, shadow, clip, scroll), rows, columns, stacks, lazy lists and grids.
- **Drawing and text:** rounded rectangles, gradients, shadows, pictures, clipping, wrapped and aligned text with weights, font families, rich spans and an ellipsis.
- **Input:** clicks, double clicks, drags, touch, scrolling, focus, keyboard, input methods, a clipboard, a text field with a blinking caret and word and line selection, and selectable text.
- **Widgets:** buttons, chips, badges, checkbox, switch, radio button, slider, progress bars, text field and text area, tabs, navigation bar, lists, dialog, popups, menus, tooltips, snackbar, animated visibility, crossfade, card and scaffold, with a light and a dark theme.
- **Animation:** `animate_*_as_state`, `Animatable`, tween and spring.
- **Navigation:** a back stack and a host.
- **Accessibility:** a semantics tree sent to the platform's screen reader API.

See [docs/architecture.md](docs/architecture.md) for the crates, what belongs in each, the frame flow, and the current limitations.

## Development

```
make fmt
make lint
```

To see what a change looks like without opening a window, draw one frame offscreen to a PNG:

```
RUSTLE_SNAPSHOT=out.png RUSTLE_SNAPSHOT_SIZE=800x600 cargo run --example hello_world
```

`RUSTLE_SNAPSHOT_LIGHT=1` renders the light theme. Any application started through `rustle::run` supports this.

To check behaviour as well as looks, replay input against the real layout before the picture is taken:

```
RUSTLE_SNAPSHOT_INPUT="click:150,237;click:230,291;type:Ada;key:Backspace" RUSTLE_SNAPSHOT=out.png cargo run --example hello_world
```

Steps are `click:x,y`, `move:x,y` (hover and pointer-move handlers), `type:text` and `key:Name` (Backspace, Delete, Enter, Tab, Left, Right, Up, Down, Home, End), separated by `;`. `RUSTLE_DEBUG_INPUT=1` prints the input a real window receives and what each click hit.
