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

It is a full-window dashboard in a restrained, monochrome style, and everything on it works: the sidebar navigates thirteen pages and collapses, Search opens a command palette, Quick Create adds a table row, the notifications and account menus act, and the dashboard's stat cards pick what the chart shows while hovering it reads out the point under the pointer. The table filters, sorts, pages, selects, hides columns and edits rows in a sheet; the overview has a working chat, calendar, role menus and invite dialog; the components page runs a transfer form against live balances, with confirmation. The Gallery page is a shadcn-style catalogue of components (buttons, inputs, OTP, tabs, pagination, tables, calendar, hover card, dialogs and more), and the Motion page shows easing curves and springs, staggered entrances, loaders, hover and click feedback, sliding tabs, an accordion, an auto-advancing carousel, swipe to dismiss, drag and drop between columns, and canvases redrawn from a clock. It follows the system's light or dark setting unless Settings overrides it. The code is split into modules under `examples/hello_world/`.

## What is here

- **Runtime:** state, `remember`, call-site identity with `key`, effects, tasks, composition locals, observable lists and maps, values saved across runs, skipping of unchanged blocks, scoped recomposition (`restartable` blocks composed again alone, with their layout cached), derived state, snapshot state (`remember`, `mutable_state_of`, `Snapshot`, `snapshot_flow`), and recomposition driven by the event loop.
- **Layout:** constraints, `Modifier` (padding, size, fill, weight, alignment, offset, aspect ratio, background, gradient, alpha, border, shadow, clip, scroll), rows, columns, stacks, lazy lists and grids, and opt-in rounding of every edge to whole logical or physical pixels (`PixelRounding`, set with `Embedded::set_pixel_rounding` or `rustle::set_pixel_rounding`).
- **Drawing and text:** rounded rectangles, linear, radial and sweep gradients, blurred drop shadows, pictures (live ones, such as video frames, made with `ImageData::keyed` under one `ImageKey`, reuse one texture), clipping, custom WGSL shaders (`ShaderBrush`), layer effects (blur, color filters, blend modes, frosted-glass backdrop blur), `draw_behind` and `draw_with_content`, wrapped and aligned text with weights, font families, rich spans and an ellipsis.
- **Input:** clicks, double clicks, drags, touch and pen, scrolling, files dropped from other applications onto file drop areas (on every platform winit 0.31 supports), focus, keyboard, input methods, a clipboard, a text field with a blinking caret and word and line selection, selectable text, rich text that draws a caret and an input method's composition when edited, and `Modifier::ime_caret` for areas that draw their own text, such as a terminal, to say where input goes.
- **Widgets:** buttons, chips, badges, checkbox, switch, radio button, slider, progress bars, text field and text area, tabs, navigation bar, lists, dialog, popups, menus, tooltips, snackbar, animated visibility, animated content with enter and exit transitions, `animate_content_size`, shared elements, crossfade, card and scaffold, with a light and a dark theme. The theme works like Compose's `MaterialTheme`: a `ColorScheme` with the Material 3 roles, a `Typography` and `Shapes`, provided by `theme(color_scheme, typography, shapes, content)` and replaceable part by part for a subtree by nesting it, with a content color that surfaces set and text follows, as Compose's `LocalContentColor`. Widgets read their colors, text styles and corner radii from it, buttons take their colors, shape, padding and border from `ButtonDefaults` or your own values through `button_custom`, a scheme can be cut from one seed color with `ColorScheme::from_seed`, shapes can be rounded, cut or per corner, surfaces take a tonal elevation, buttons ripple, and extra values go in a composition local of your own, as Compose's custom design systems guide shows. `cargo run --release --example theme` shows it.
- **Animation:** `animate_*_as_state`, `Animatable` (with `animate_decay` and fling), `update_transition` for several values off one state, tween, spring, keyframes, snap, repeatable and infinite repeatable specs, and `Path` morphing through `Lerp`.
- **Navigation:** a back stack and a host.
- **Accessibility:** a semantics tree sent to the platform's screen reader API through AccessKit (AT-SPI, UI Automation and `NSAccessibility`).
- **Embedding:** `rustle::Embedded` runs an interface inside a host that owns the window, the GPU device and the frame. The host gives it the size, the pointer, the wheel, the keys and the input method, learns from each whether the interface used the event (`Modifier::blocks_pointer` makes a panel's empty space count), draws it into its own texture, can replace the root without rebuilding the renderer, backs `remember_saveable` with its own `Saver`, and draws its own content into an area the interface reserves with `viewport`, and can time frames with its own clock (`set_clock`, `advance_to`) so offscreen snapshots taken at a given moment are the same every run.

See [docs/architecture.md](docs/architecture.md) for the crates, what belongs in each, the frame flow, and the current limitations.

## Development

```
make fmt
make lint
```

`cargo run --example embedded -p rustle` checks the embedding API without a window: it clicks, types through an input method, reads the caret an area that draws its own text reports, replaces the root and draws two interfaces into textures it made.

To measure animated-frame CPU work and offscreen rendering in release mode:

```sh
BENCH_ROWS=600 BENCH_FRAMES=300 cargo run --release -p rustle --example bench
```

The benchmark compares ordinary rows, skippable rows and `restartable` blocks (the changing bar and the list recomposed alone, the list's layout reused) in a non-lazy scrolling list at 900×700, after five warmup frames. It reports composition and layout/paint medians, render-plus-readback time, and measured total-frame median/p95/p99. The rendering stage includes GPU synchronization and pixel readback, not just GPU execution; these are not native-window FPS measurements. Increase `BENCH_ROWS` to check scaling, use a positive `BENCH_FRAMES`, and repeat runs to account for other work on the machine. For application lists that should only compose visible items, use the lazy list APIs instead. `BENCH_SCENARIO=effects` (or `list`) runs one scenario only: a grid of rounded clips, effect layers and backdrop blurs drawn straight to the renderer, sized with `BENCH_CARDS` and `BENCH_SIZE` (for example `1920x1080`).

To see what a change looks like without opening a window, draw one frame offscreen to a PNG:

```
RUSTLE_SNAPSHOT=out.png RUSTLE_SNAPSHOT_SIZE=800x600 cargo run --example hello_world
```

`RUSTLE_SNAPSHOT_LIGHT=1` renders the light theme. Any application started through `rustle::run` supports this.

`RUSTLE_SNAPSHOT_SCALE=2` (or `1.25`, `1.5`, ...) draws the picture at that scale factor, as a HiDPI screen would: the layout keeps its logical size and the picture has that many physical pixels per logical pixel.

To check behaviour as well as looks, replay input against the real layout before the picture is taken:

```
RUSTLE_SNAPSHOT_INPUT="click:150,237;click:230,291;type:Ada;key:Backspace" RUSTLE_SNAPSHOT=out.png cargo run --example hello_world
```

Steps are `click:x,y`, `move:x,y` (hover and pointer-move handlers), `type:text` and `key:Name` (Backspace, Delete, Enter, Tab, Left, Right, Up, Down, Home, End), separated by `;`. Each step is followed by a second of frames, so animations have finished; `RUSTLE_SNAPSHOT_STEP_MS=0` takes the picture right after the last step, and a `wait:ms` step runs that many milliseconds of frames first, to catch an animation midway (`click:150,237;wait:120`). `RUSTLE_DEBUG_INPUT=1` prints the input a real window receives and what each click hit.
