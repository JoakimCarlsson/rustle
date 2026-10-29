# Architecture

Rustle is split into layers. Each crate depends only on the ones below it, and a renderer depends on nothing above `rustle-graphics`.

## Crates

| Crate | Contains | Does not contain |
| --- | --- | --- |
| `rustle-runtime` | `State`, `remember`, `Composition` (slots, invalidation, full-root recomposition) | UI types, layout, rendering |
| `rustle-unit` | `dp`, `sp`, size constraints. Empty for now | Layout algorithms, rendering |
| `rustle-geometry` | Offsets, sizes, rectangles. Empty for now | Colors, drawing |
| `rustle-graphics` | `DrawCommand`, fixed text metrics: the contract a renderer implements | Layout, composition, any GPU or window code |
| `rustle-text` | Text styles, fonts, measurement (the interface layout uses to size text), later editing. Empty for now | Glyph rendering, anything GPU-specific |
| `rustle-ui` | `Node` tree, scope handling (`emit`, `collect`, `compose`), `composition()`, layout | Concrete layouts and elements as public API, rendering |
| `rustle-animation` | Animated values, easing, springs, transitions, driven by a frame clock. Empty for now | Layout, drawing, rendering |
| `rustle-foundation` | Design-system-agnostic composables: `column`, `text`; later `row`, `box`, modifiers like `clickable`, lazy lists, `text_field` | Styling, theming |
| `rustle-widgets` | Styled components (`button`, `checkbox`) and theming. Empty for now | Layout primitives |
| `rustle-macros` | `#[composable]` (a pass-through for now) | Runtime logic |
| `rustle-shaders` | WGSL source strings (`QUAD`, `GLYPH`, `IMAGE`) | Pipelines or GPU code |
| `rustle-wgpu` | wgpu renderer: surface, text through glyphon, drawing `DrawCommand`s | Composition, state, the UI tree |
| `rustle` | Public API, prelude, winit window and event loop (`run`) | Anything reusable by other platforms |

## Dependencies

```
rustle-unit ──────┐
rustle-geometry ──┼─> rustle-text ─┐
                  │                │
rustle-runtime ───┼────────────────┼─> rustle-ui ─> rustle-foundation ─> rustle-widgets
rustle-graphics ──┘                │
                                   │
rustle-runtime ─> rustle-animation
rustle-graphics ─┐
rustle-shaders  ─┴─> rustle-wgpu

rustle  ─> every crate above
```

Today only some of these edges exist, because the empty crates have no code to use yet. `rustle-text` already depends on unit and geometry. `rustle-ui` and `rustle-graphics` will depend on the new crates as soon as layout takes constraints and `DrawCommand` takes geometry.

The runtime never sees a `Node` and the renderer never sees a `State` or the tree.

## Frame flow

1. `rustle::run` creates a `Composition` through `rustle_ui::composition(root)`.
2. A pass runs the root composable. `text` and `column` push nodes into the current scope and `remember` reads or creates state in the composition's slots. The pass returns a `Node` tree.
3. `rustle_ui::layout` turns the tree into `DrawCommand`s.
4. `rustle_wgpu::Renderer::render` draws them.
5. `State::set` and `State::update` only mark the composition dirty. The first clean-to-dirty change sends a winit user event, and the event loop then composes the whole root again (steps 2 to 4). Several updates before that event cost one pass.

## Where new things go

- A composable that only arranges or draws children, such as `row`, `box` or a lazy list: `rustle-foundation`.
- Something every composable can take, such as `Modifier`, constraints and measurement: `rustle-ui`, because modifiers and layout depend on each other.
- A unit, size constraint or geometry type: `rustle-unit` or `rustle-geometry`.
- Anything about how text looks or is measured: `rustle-text`. The renderer implements the measuring interface that layout calls.
- A new kind of drawing instruction: `rustle-graphics`, then support in each renderer.
- Animated values, easing, springs and transitions: `rustle-animation`. It needs the runtime's effects and frame clock first, and depends on `rustle-ui` once it animates modifiers.
- A component with a look, such as `button`, or theme values: `rustle-widgets`.
- A new renderer backend (Skia, Vello): its own crate depending on `rustle-graphics`.
- Platform integration for another OS or toolkit: alongside `rustle`, not in the layers above.

## Current limitations

- `Node` is a closed enum (`Column`, `Text`), so `column` and `text` in foundation still build `rustle-ui`'s enum variants. The plan is one generic layout node with a measure policy in `rustle-ui`, with `column`, `row` and `box` becoming policies in foundation.
- Remembered state is matched by call order within a composition. A pass that changes the number, order or type of `remember` calls panics. Conditionals, keys and dynamic lists need `key`/group support first.
- State is single-threaded (`Rc`).
- Layout is a fixed-size vertical column. There are no modifiers, constraints or text measurement yet.
