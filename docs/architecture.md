# Architecture

Rustle is split into layers. Each crate depends only on the ones below it, and a renderer depends on nothing above `rustle-graphics`.

## Crates

| Crate | Contains | Does not contain |
| --- | --- | --- |
| `rustle-runtime` | `State`, `remember`, `Composition` (slots, invalidation, full-root recomposition) | UI types, layout, rendering |
| `rustle-unit` | `Dp`, `Constraints` (what a parent lets a child measure to) | Layout algorithms, rendering |
| `rustle-geometry` | `Offset`, `Size`, `Rect` | Colors, drawing |
| `rustle-graphics` | `Color` and `DrawCommand` (`Rect`, `Text`): the contract a renderer implements | Layout, composition, any GPU or window code |
| `rustle-text` | `TextStyle` and the `TextMeasurer` interface layout uses to size text; later fonts and editing | Glyph rendering, anything GPU-specific |
| `rustle-ui` | `Node`, `Modifier` (including `clickable`), the `Layout` policy trait, `layout()`/`emit`/`collect` for composables, `composition()`, and `display_list` (measure, then paint into `DrawCommand`s and clickable regions) | Concrete layouts such as column and row, rendering |
| `rustle-animation` | Animated values, easing, springs, transitions, driven by a frame clock. Empty for now | Layout, drawing, rendering |
| `rustle-foundation` | `text`, `column`, `row`, `stack`, `spacer`, each with a `_with(modifier, ..)` form; later lazy lists, `text_field` | Styling, theming |
| `rustle-widgets` | Styled components: `button`; later `checkbox` and theming | Layout primitives |
| `rustle-macros` | `#[composable]` (a pass-through for now) | Runtime logic |
| `rustle-shaders` | WGSL source strings (`QUAD`, `GLYPH`, `IMAGE`) | Pipelines or GPU code |
| `rustle-wgpu` | wgpu renderer: surface, quads through `rustle-shaders`, text through glyphon, and the `TextMeasurer` implementation | Composition, state, the UI tree |
| `rustle` | Public API, prelude, winit window and event loop (`run`), including turning pointer events into clicks | Anything reusable by other platforms |

## Dependencies

```
rustle-geometry ─> rustle-unit ─────────────┐
rustle-geometry ─> rustle-graphics ─────────┤
rustle-geometry ─> rustle-text ─────────────┼─> rustle-ui ─> rustle-foundation ─> rustle-widgets
rustle-runtime ─────────────────────────────┘

rustle-runtime ─> rustle-animation

rustle-graphics ─┐
rustle-text     ─┼─> rustle-wgpu
rustle-shaders  ─┘

rustle  ─> every crate above
```

`rustle-foundation` also uses unit, geometry, graphics and text directly. `rustle-animation` is empty for now and will depend on `rustle-ui` once it animates modifiers.

The runtime never sees a `Node` and the renderer never sees a `State` or the tree.

## Frame flow

1. `rustle::run` creates a `Composition` through `rustle_ui::composition(root)`.
2. A pass runs the root composable. `text`, `column` and the other composables push `Node`s into the current scope and `remember` reads or creates state in the composition's slots. The pass returns a `Node` tree.
3. `rustle_ui::display_list` measures the tree within the window's constraints. Each node applies its modifiers from the outermost in, then its `Layout` policy measures the children. Text is measured by the renderer through `TextMeasurer`. The measured tree is then painted into `DrawCommand`s.
4. `rustle_wgpu::Renderer::render` draws them: all rectangles first, then all text.
5. Mouse events are hit-tested against the clickable regions of the latest layout. A press and a release on the same region run its handler, which normally changes state.
6. `State::set` and `State::update` only mark the composition dirty. The first clean-to-dirty change sends a winit user event, and the event loop then composes the whole root again and lays it out again (steps 2 to 4). Several updates before that event cost one pass. Resizing the window or changing its scale also lays out again, without composing.

## Where new things go

- A composable that only arranges or draws children, such as `row`, `box` or a lazy list: `rustle-foundation`.
- Something every composable can take, such as `Modifier`, constraints and measurement: `rustle-ui`, because modifiers and layout depend on each other.
- A unit, size constraint or geometry type: `rustle-unit` or `rustle-geometry`.
- Anything about how text looks or is measured: `rustle-text`. The renderer implements the measuring interface that layout calls.
- A new kind of drawing instruction: `rustle-graphics`, then support in each renderer.
- A new layout, such as a grid: implement `Layout` in `rustle-foundation`.
- Animated values, easing, springs and transitions: `rustle-animation`. It needs the runtime's effects and frame clock first, and depends on `rustle-ui` once it animates modifiers.
- A component with a look, such as `button`, or theme values: `rustle-widgets`.
- A new renderer backend (Skia, Vello): its own crate depending on `rustle-graphics`.
- Platform integration for another OS or toolkit: alongside `rustle`, not in the layers above.

## Current limitations

- Remembered state is matched by call order within a composition. A pass that changes the number, order or type of `remember` calls panics. Conditionals, keys and dynamic lists need `key`/group support first.
- State is single-threaded (`Rc`).
- Text is one line with a fixed style and color. There is no wrapping or alignment.
- All rectangles paint before all text, so a rectangle can never cover text drawn earlier in the tree.
- Modifiers: `padding`, `size`, `width`, `height`, `fill_max_*` and `background`. There is no border, alignment or weight yet.
- Input is the primary mouse button on `clickable` only. There is no hover or pressed feedback, keyboard, focus or scrolling.
