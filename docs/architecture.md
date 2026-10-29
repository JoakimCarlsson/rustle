# Architecture

Rustle is split into layers. Each crate depends only on the ones below it, and a renderer depends on nothing above `rustle-graphics`.

## Crates

| Crate | Contains | Does not contain |
| --- | --- | --- |
| `rustle-runtime` | `State`, `Composition` (the group tree, full-root recomposition), `remember`, `group`, `key`, `memo` (skips a block whose inputs and reads are unchanged), `side_effect`, `disposable_effect`, `launched_effect` and `produce_state` (tasks on a frame-driven executor), `remember_updated_state`, `derived_state_of`, `remember_saveable` (values kept across runs, through a host-provided `Saver`), `StateList` and `StateMap`, `CompositionLocal`, the frame clock | UI types, layout, rendering |
| `rustle-unit` | `Dp`, `Constraints` (what a parent lets a child measure to) | Layout algorithms, rendering |
| `rustle-geometry` | `Offset`, `Size`, `Rect` | Colors, drawing |
| `rustle-graphics` | `Color`, `ImageData` (with PNG/JPEG decoding and encoding), `Path` (a filled vector outline of `PathCommand`s, constant data cached by renderers), `DrawCommand` (`Rect`, `Gradient`, `Arc`, `Line`, `Shadow`, `Path`, `Text`, `RichText`, `Image`, `PushClip`, `PopClip`, `PushLayer`, `PopLayer`): the contract a renderer implements | Layout, composition, any GPU or window code |
| `rustle-text` | `TextStyle` (color, size, weight, font family, alignment, line limit), `TextSpan` for rich text, `FontWeight`, and the `TextMeasurer` interface layout uses to size text | Glyph rendering, anything GPU-specific |
| `rustle-ui` | `Node`, `canvas` and `Canvas` (an area drawn with shapes, vector paths, lines and text in its own coordinates), `Modifier` (including gradient, alpha, border, shadow, clip, z-index, ring, graphics layers (rotate, scale, group alpha), file drops, drag sources and drop targets, pinch, scroll, hover, long press, size callbacks, input and semantics), the `Layout` policy trait, `Overlay`, scrolling, keyboard input and focus, semantics for assistive technology, `layout()`/`emit`/`collect` for composables, `skippable`, `composition()`, and `display_list` (measure, then paint into `DrawCommand`s and input, scroll and semantics regions) | Concrete layouts such as column and row, rendering |
| `rustle-animation` | `animate_as_state` (and float and color forms), `animate_value`, `Animatable`, `infinite_float`, `AnimationSpec` (`tween`, `spring`), `Easing`, `Lerp` | Layout, drawing, rendering |
| `rustle-foundation` | `text`, `column`, `row`, `stack`, `spacer`, `image`, each with a `_with(modifier, ..)` form; `column_arranged`/`row_arranged` with `Arrangement` and `Alignment`; `rich_text`; `flow_row`, `floating` and `reveal`; `lazy_column`/`lazy_row`/`lazy_vertical_grid`, and `lazy_list` for items of different heights; `basic_text_field`, `basic_text_area` and `selectable_text`; `LOCAL_TEXT_STYLE` and the system appearance | Styling, theming |
| `rustle-widgets` | `Theme` (a monochrome light and dark palette, as a composition local), `icon` (draws an `Icon`, a Phosphor outline such as `phosphor::PULSE`), `styled_button` and `ButtonStyle`, `pill`, `avatar`, `select_field`, `segmented_control`, `tabs_list`, `table_row`/`table_cell`, `calendar`, `card`, `checkbox`, `switch`, `radio_button`, `slider`, `range_slider`, `text_field`, `text_area`, `dialog`, `alert_dialog`, `bottom_sheet`, `popup`, `dropdown_menu`, `menu`/`MenuItem` (a trigger that opens a menu kept inside the window), `select`, `tooltip`, `snackbar`, `horizontal_pager`, `swipe_to_dismiss`, `animated_visibility`, `crossfade`, and others | Layout primitives |
| `rustle-charts` | `area_chart`, `line_chart`, `bar_chart`, `donut_chart`, `sparkline` and `qr_code`, plus `chart_hover` and `bar_hover` for reading out the point under the pointer, drawn on a canvas in colors they are given | Anything that depends on a theme |
| `rustle-navigation` | `NavController` (a back stack), `nav_host` | Screens, rendering |
| `rustle-macros` | `#[composable]`, which wraps a function body in a call-site group | Runtime logic |
| `rustle-icons` | `Icon` (a `Path`) and `phosphor`: every Phosphor icon (MIT, license in `LICENSE-PHOSPHOR`) as a constant, like `phosphor::ARROW_UP_RIGHT`, so names are checked and completed by the compiler and icons nothing uses are not in the program. Regular weight always; `thin`, `light`, `bold` and `fill` are features that add `phosphor::bold::BELL` and so on (not duotone) | Rendering, widgets |
| `rustle-icons-macros` | The proc macro that turns the vendored SVGs it holds into those constants | Anything but the macro |
| `rustle-shaders` | WGSL source strings (`QUAD`, `GLYPH`, `IMAGE`, `COMPOSITE`, `PATH`) | Pipelines or GPU code |
| `rustle-wgpu` | wgpu renderer: surface, quads, pictures and vector paths (tessellated with lyon, edges antialiased with a fringe, meshes of constant paths cached) through `rustle-shaders`, text through glyphon, clipping, layers drawn to textures and composited turned, scaled and faded, and the `TextMeasurer` implementation | Composition, state, the UI tree |
| `rustle` | Public API, prelude, the winit window and event loop (`run`), and `preview` (a composable drawn to a PNG with no window): turns pointer, touch, wheel, keyboard and input-method events into clicks, drags, scrolls and key events, counts repeated presses, keeps saved values in a file, runs frames, and talks to the platform's accessibility API | Anything reusable by other platforms |

## Dependencies

```
rustle-geometry ─> rustle-unit ─────────────┐
rustle-geometry ─> rustle-graphics ─────────┤
rustle-geometry ─> rustle-text ─────────────┼─> rustle-ui ─> rustle-foundation ─> rustle-widgets
rustle-runtime ─────────────────────────────┘

rustle-runtime  ─┐
rustle-graphics ─┴─> rustle-animation ─> rustle-widgets
rustle-runtime ─> rustle-navigation
rustle-ui ─> rustle-charts

rustle-graphics ─> rustle-icons ─> rustle-widgets
rustle-icons-macros ─> rustle-icons

rustle-graphics ─┐
rustle-text     ─┼─> rustle-wgpu
rustle-shaders  ─┘

rustle  ─> every crate above
```

`rustle-foundation` also uses runtime, unit, geometry, graphics and text directly. `rustle-animation` animates plain values, so it does not depend on `rustle-ui`.

The runtime never sees a `Node` and the renderer never sees a `State` or the tree.

## Frame flow

1. `rustle::run` creates a `Composition` through `rustle_ui::composition(root)`.
2. A pass runs the root composable. `text`, `column` and the other composables push `Node`s into the current scope. Every composable call site opens a group, and `remember` reads or creates state in its group. The pass returns a `Node` tree. Effects the pass scheduled run when it has finished.
3. `rustle_ui::display_list` measures the tree within the window's constraints. Each node applies its modifiers from the outermost in, then its `Layout` policy measures the children. Text is measured by the renderer through `TextMeasurer`. The measured tree is then painted into `DrawCommand`s plus the clickable, pressable, draggable, scrollable, focusable and described regions.
4. `rustle_wgpu::Renderer::render` draws the commands in order. Consecutive rectangles are drawn together, then the text or pictures that follow, so later commands paint over earlier ones, each cut to the clip in effect.
5. Animations and suspending tasks ask the runtime's frame clock for the next frame. The first request wakes the event loop, which then redraws each frame, runs the requested callbacks (they change state and wake tasks), composes and lays out again, and draws. When nothing is left to run, no more frames are requested.
6. Input is hit-tested against the regions of the latest layout. Areas are told apart by where they are and how big, never by their handlers, because every recomposition makes new handlers (and an animation recomposes every frame), so a press is matched to its release by area and the handler that runs is the current one. A press and a release on the same clickable runs its handler; a press focuses the focusable under it and runs press and drag handlers, and a drag keeps going while the button is down; the wheel scrolls the innermost scrolling area under the pointer; key events go to the focused area, and unhandled Tab and Escape move or drop focus.
7. `State::set` and `State::update` only mark the composition dirty. The first clean-to-dirty change sends a winit user event, and the event loop then composes the whole root again and lays it out again (steps 2 to 4). Several updates before that event cost one pass. Resizing the window or changing its scale also lays out again, without composing.
8. `RUSTLE_SNAPSHOT=out.png` (or `rustle::preview`) runs steps 2 to 4 once against an offscreen renderer and writes the frame as a PNG, which is how the layout of a composable can be checked without a window.
9. After each layout the described regions are sent to the platform's accessibility API, and its requests to click or focus are run.

## Identity and state

Every `#[composable]` function, layout (`column`, `row`, `stack`, `button`, ...) and `remember` call opens a group identified by its source location, using `#[track_caller]`. State belongs to its group, so:

- a call inside an untaken `if` leaves the composition, drops its state and runs its cleanups, without disturbing its siblings;
- calls from the same site, such as in a loop, are matched by order. Wrap each item in `key(id, || ...)` if items can be inserted, removed or reordered;
- groups the root did not reach in a pass are dropped after it, outside any borrow.

Effects run after the pass: `side_effect` every pass, `disposable_effect` when its key changes, with a cleanup before the next run and when it leaves the composition, and `launched_effect` starts a task that waits on frames and time, cancelled when its key changes or it leaves. `CompositionLocal` passes values down the tree, and an inner `provide` overrides an outer one.

## Skipping

`#[composable(skippable)]` and `skippable(deps, || ...)` skip their body when called again with equal `deps` and when none of the state or composition locals read during the last run has changed. State reads are recorded with the version they had; locals are recorded as a check that the value is still equal. A skipped block keeps what it remembered and re-emits the nodes it produced. Effects inside it do not run again.

## Where new things go

- A composable that only arranges or draws children, such as `row`, `box` or a lazy list: `rustle-foundation`.
- Something every composable can take, such as `Modifier`, constraints and measurement: `rustle-ui`, because modifiers and layout depend on each other.
- A unit, size constraint or geometry type: `rustle-unit` or `rustle-geometry`.
- Anything about how text looks or is measured: `rustle-text`. The renderer implements the measuring interface that layout calls.
- A new kind of drawing instruction: `rustle-graphics`, then support in each renderer.
- A new layout, such as a grid: implement `Layout` in `rustle-foundation`.
- Animated values, easing, springs and transitions: `rustle-animation`, built on the runtime's frame clock and tasks.
- A component with a look, such as `button`, or theme values: `rustle-widgets`.
- Screens and moving between them: `rustle-navigation`.
- A new renderer backend (Skia, Vello): its own crate depending on `rustle-graphics`.
- Platform integration for another OS or toolkit: alongside `rustle`, not in the layers above.

## Current limitations

- State is single-threaded (`Rc`), and `State` handles are not `Copy`, so closures need clones. Tasks wait on frames and time, not on other threads.
- The root is composed again on every state change. Blocks marked `skippable` (or `#[composable(skippable)]`) are skipped when their arguments are equal and nothing they read has changed, but the parent that calls them still runs. There is no snapshot state, so `derived_state_of` only gives a result the type of state.
- `lazy_column`, `lazy_row` and `lazy_vertical_grid` need items of one fixed size; `lazy_list` takes any heights but estimates the ones it has not laid out yet.
- Text fields do not wrap long lines: they scroll to keep the caret in view, and multi-line fields end lines only at line breaks. Text is set with one size and line height per node, and rich text varies only color, weight and family.
- Clips are rectangular. `Modifier::alpha` fades each thing on its own; use `graphics_layer` for group-correct fading, rotation and scale, which draws the content to a texture the size of the window. Pointer input in a rotated or scaled area still goes where the area was laid out. Raised layers (`z_index`) are not clipped by their parents.
- Input is the primary mouse button (with double and triple click counts), a touch contact treated as the same pointer, long presses, hover, the wheel, the keyboard and input-method text. Pinch gestures, files dropped from other applications, and dragging content from a `drag_source` to a `drop_target` inside the application are supported. There is no multi-finger input beyond pinch, and nothing can be dragged out of the window.
- Accessibility describes a flat list of areas with their role, label, value and state, and supports click and focus requests.
- Right-to-left layout is supported by providing `LOCAL_LAYOUT_DIRECTION`: rows, stacks, flows and start-aligned columns mirror, text starts on its own side, and `padding_directional` swaps. Scroll directions, sliders and other widgets do not mirror, and text is not localized.
