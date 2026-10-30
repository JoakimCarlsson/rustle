# Architecture

Rustle is split into layers. Each crate depends only on the ones below it, and a renderer depends on nothing above `rustle-graphics`.

## Crates

| Crate | Contains | Does not contain |
| --- | --- | --- |
| `rustle-runtime` | `State`, `Composition` (the group tree, full-root recomposition), `remember`, `group`, `key`, `memo` (skips a block whose inputs and reads are unchanged), `side_effect`, `disposable_effect`, `launched_effect` and `produce_state` (tasks on a frame-driven executor), `remember_updated_state`, `derived_state_of`, `remember_saveable` (values kept across runs, through a host-provided `Saver`), `StateList` and `StateMap`, `CompositionLocal`, the frame clock | UI types, layout, rendering |
| `rustle-unit` | `Dp`, `Constraints` (what a parent lets a child measure to) | Layout algorithms, rendering |
| `rustle-geometry` | `Offset`, `Size`, `Rect` | Colors, drawing |
| `rustle-graphics` | `Color`, `ImageData` (with PNG/JPEG decoding and encoding), `Path` (a filled vector outline of `PathCommand`s, constant data cached by renderers), `DrawCommand` (`Rect`, `Gradient`, `Arc`, `Line`, `Shadow`, `Path`, `Text`, `RichText`, `Image`, `PushClip`, `PushRoundedClip`, `PopClip`, `PushLayer`, `PopLayer`): the contract a renderer implements | Layout, composition, any GPU or window code |
| `rustle-text` | `TextStyle` (color, size, weight, font family, alignment, line limit, italic, underline, strikethrough, letter spacing, ligatures, `TextWrap`), `TextSpan` for rich text (the same, per run, plus a background and a size), `FontWeight`, the `TextMeasurer` interface layout uses to size text, `TextLayout` (shaped text as plain data: visual lines with metrics, clusters, caret position, hit testing, vertical movement, selection rectangles, background and underline rectangles), and the functions `layout_text`, `layout_spans`, `font_families` and `register_font`, which reach the `TextEngine` a renderer installs | Glyph rendering, anything GPU-specific |
| `rustle-ui` | `Node`, `canvas` and `Canvas` (an area drawn with shapes, vector paths, lines and text in its own coordinates; `draw_text` and `draw_rich_text` draw wrapped, styled text and return its `TextLayout`, and `selection` and `caret` draw over it), `Modifier` (including `cursor` with `CursorIcon`, gradient, alpha, border, shadow, clip, z-index, `clip_rounded` and `clip_rounded_corners` (antialiased, for everything inside, nestable), `z_index` and `z_index_unclipped`, ring, graphics layers (rotate, scale, group alpha), file drops, drag sources and drop targets, pinch, scroll, hover, long press, size callbacks, input and semantics), the `Layout` policy trait, `Overlay`, scrolling (`ScrollState` with offset, maximum, viewport and content length, plus an overlay scrollbar that fades, can be dragged, and is set with `ScrollbarStyle`), keyboard input and focus, semantics for assistive technology, `layout()`/`emit`/`collect` for composables, `skippable`, `composition()`, and `display_list` (measure, then paint into `DrawCommand`s and input, scroll and semantics regions) | Concrete layouts such as column and row, rendering |
| `rustle-animation` | `animate_as_state` (and float and color forms), `animate_value`, `Animatable`, `infinite_float`, `AnimationSpec` (`tween`, `spring`), `Easing`, `Lerp` | Layout, drawing, rendering |
| `rustle-foundation` | `text`, `column`, `row`, `stack`, `spacer`, `image`, each with a `_with(modifier, ..)` form; `column_arranged`/`row_arranged` with `Arrangement` and `Alignment`; `rich_text`; `flow_row`, `floating` and `reveal`; `lazy_column`/`lazy_row`/`lazy_vertical_grid`, and `lazy_list`/`lazy_list_keyed` for items of different heights (exact offsets once measured, the visible item kept in place as items above are measured or inserted, `set_stick_to_end` for chat-style lists, `scroll_to_item` and `scroll_to_end`); `basic_text_field`, `basic_text_area` (wrapping), `basic_text_area_no_wrap`, `selectable_text`, and `selection_container` (one selection, with copy, across the `selectable_text` and `rich_text` blocks inside it); `LOCAL_TEXT_STYLE` and the system appearance | Styling, theming |
| `rustle-widgets` | `Theme` (a monochrome light and dark palette, as a composition local), `icon` (draws an `Icon`, a Phosphor outline such as `phosphor::PULSE`), `styled_button` and `ButtonStyle`, `pill`, `avatar`, `select_field`, `segmented_control`, `tabs_list`, `table_row`/`table_cell`, `calendar`, `card`, `checkbox`, `switch`, `radio_button`, `slider`, `range_slider`, `text_field`, `text_area`, `dialog`, `alert_dialog`, `bottom_sheet`, `popup`, `dropdown_menu`, `menu`/`MenuItem` (a trigger that opens a menu kept inside the window), `select`, `tooltip`, `snackbar`, `horizontal_pager`, `swipe_to_dismiss`, `animated_visibility`, `crossfade`, and others | Layout primitives |
| `rustle-charts` | `area_chart`, `line_chart`, `bar_chart`, `donut_chart`, `sparkline` and `qr_code`, plus `chart_hover` and `bar_hover` for reading out the point under the pointer, drawn on a canvas in colors they are given | Anything that depends on a theme |
| `rustle-navigation` | `NavController` (a back stack), `nav_host` | Screens, rendering |
| `rustle-macros` | `#[composable]`, which wraps a function body in a call-site group | Runtime logic |
| `rustle-icons` | `Icon` (a `Path`) and `phosphor`: every Phosphor icon (MIT, license in `LICENSE-PHOSPHOR`) as a constant, like `phosphor::ARROW_UP_RIGHT`, so names are checked and completed by the compiler and icons nothing uses are not in the program. Regular weight always; `thin`, `light`, `bold` and `fill` are features that add `phosphor::bold::BELL` and so on (not duotone) | Rendering, widgets |
| `rustle-icons-macros` | The proc macro that turns the vendored SVGs it holds into those constants | Anything but the macro |
| `rustle-shaders` | WGSL source strings (`QUAD`, `GLYPH`, `IMAGE`, `COMPOSITE`, `PATH`) | Pipelines or GPU code |
| `rustle-wgpu` | wgpu renderer: surface, quads, pictures and vector paths (tessellated with lyon, edges antialiased with a fringe, meshes of constant paths cached) through `rustle-shaders`, text through glyphon, clipping (rectangular clips are scissors; rounded clips draw their content to a texture and composite it through an antialiased rounded-rectangle mask, so images, text and outlines are cut alike and clips nest), layers drawn to textures (reused between frames) and composited turned, scaled and faded, commands outside the clip skipped before they are shaped or uploaded, the `TextMeasurer` implementation (sizes cached between frames), and the `TextEngine` (glyph-level layout, installed system fonts, fonts registered at run time) | Composition, state, the UI tree |
| `rustle` | Public API, prelude, the winit event loop (`run`, `run_with`, or `Host` inside a loop the caller owns), windows (`WindowOptions`; `window`, a composable that opens a native window while it is in the composition; `WindowHandle` for drag, resize, minimize, maximize, close and title; per-window renderer, size, scale, pointer, focus and display list), and `preview` (a composable drawn to a PNG with no window): turns pointer, touch, wheel, keyboard and input-method events into clicks, drags, scrolls and key events, counts repeated presses, keeps saved values in a file, runs frames, and talks to the platform's accessibility API | Anything reusable by other platforms |

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
4. `rustle_wgpu::Renderer::render` draws the commands in order. Consecutive rectangles are drawn together, then the text or pictures that follow, so later commands paint over earlier ones, each cut to the clip in effect. Commands that lie wholly outside the clip are dropped first, so a long column inside a scrolling area costs what is visible.
5. Animations and suspending tasks ask the runtime's frame clock for the next frame. The first request wakes the event loop, which then redraws each frame, runs the requested callbacks (they change state and wake tasks), composes and lays out again, and draws. When nothing is left to run, no more frames are requested.
6. Input is hit-tested against the regions of the latest layout. Areas are told apart by where they are and how big, never by their handlers, because every recomposition makes new handlers (and an animation recomposes every frame), so a press is matched to its release by area and the handler that runs is the current one. A press and a release on the same clickable runs its handler; a press focuses the focusable under it and runs press and drag handlers, and a drag keeps going while the button is down; the wheel scrolls the innermost scrolling area under the pointer.

   Pointer buttons: the primary button (or a touch) drives clickables, focus, drags and long presses as before. Every button, the primary one included, can also be listened to with `Modifier::on_pointer_button(button, ..)` and its shorthands (`on_secondary_click`, `on_context_menu`, `on_middle_click`, `on_back_click`, `on_forward_click`, `on_primary_click`, `on_button_drag`). Those areas are hit-tested per button: the topmost area that listens to the button gets it, and a region for one button never blocks the others, so a parent's `on_context_menu` works over a child's `clickable`. Each `PointerEvent` says which button, the phase (`Press`, `Drag` while held, `Release`, `Click` when released on the area it went down on), the position in the area and in the window, the modifiers held, and the click count.

   Keys: a key event (`KeyEvent`) carries the logical `Key` (with `F(n)`, `Insert`, `PrintScreen`, `ContextMenu`, lock and modifier keys), the `PhysicalKey` (a `KeyCode` named like the W3C `code`, independent of layout), the `KeyLocation`, `repeat`, and `pressed` (false for a release). It is given, stopping at the first handler that returns true, to: the `on_preview_key_event` areas around the focused area (outermost first), the focused area's `focusable` handler, then the `on_key_event` areas around it (innermost first). With nothing focused every key area in the window is around it, so a root `on_key_event` holds the application's shortcuts. Only when none handled it do Tab and Escape move or drop focus. `KeyEvent::is` matches a logical key plus exact modifiers and `KeyEvent::is_code` a physical one.

   Input methods: winit's `Ime::Preedit` is stored on the focused `Focus` (`Focus::preedit`). Text fields insert the composition at the caret, underline it, and keep it out of their value until `Ime::Commit` arrives as typed text. While a field is being edited, layout reports its caret rectangle in `DisplayList::ime_area`, and the window passes it to `set_ime_cursor_area` when it moves, so the candidate popup follows the caret. The window allows IME input while something has focus.
7. `State::set` and `State::update` only mark the composition dirty. The first clean-to-dirty change sends a winit user event, and the event loop then composes the whole root again and lays it out again (steps 2 to 4). Several updates before that event cost one pass. Resizing the window or changing its scale also lays out again, without composing.
8. `RUSTLE_SNAPSHOT=out.png` (or `rustle::preview`) runs steps 2 to 4 once against an offscreen renderer and writes the frame as a PNG, which is how the layout of a composable can be checked without a window. `RUSTLE_SNAPSHOT_INPUT` replays input first, as `;`-separated steps: `click:x,y`, `rclick:x,y`, `mclick:x,y`, `backclick:x,y`, `forwardclick:x,y`, `move:x,y`, `type:text`, `key:Name` (logical, such as `F5`, `Escape`, `a`), `code:KeyA` (physical), `release:Name`, `mod:ctrl+shift` (modifiers held for the steps after; `mod:none` clears), `preedit:text` and `commit:text` (an input method composing and committing in the focused field).
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

## Windows

The root composable is shown in the main window. `window(options, on_close_request, content)` composes `content` inside the same pass, so it shares state, composition locals and the theme with its parent, but the resulting tree is not emitted into the parent: it is stored in a per-thread registry under a key remembered in the call's group, and a `disposable_effect` removes the entry when the call leaves the composition. The event loop looks at the registry when it idles (`about_to_wait`, and after each window event): new entries get a native window, renderer and accessibility adapter, removed ones are dropped, changed titles and other live options are applied. Events are routed by `WindowId` (`Host::window_event` takes it). The input code is written against one window's fields on `App`, so the fields of the window being served are swapped in from `App::slots` when its event arrives (`crate::windows`); after composing, every window is laid out again from its own tree. `WindowHandle::drag_window` and `drag_resize_window` hand the pointer to the system, which never sends the release, so the press state of that window is cleared afterwards.

## Current limitations

- Windows are only opened and closed from the event loop, so a window appears after the pass that asked for it, not inside it. A secondary window never closes itself: its `on_close_request` must stop the `window` call. The main window cannot be opened or closed from composition. Window transparency, per-window themes, window positions, fullscreen, modal and parent/child relationships are not supported, and `preview` draws only the root. Keyboard focus is tracked per window, but a window that loses OS focus keeps showing its focused control as focused.
- State is single-threaded (`Rc`), and `State` handles are not `Copy`, so closures need clones. Tasks wait on frames and time, not on other threads.
- The root is composed again on every state change, then laid out and painted from scratch; there are no scoped recompositions and no cache of the layout or paint of unchanged subtrees. The cost is mostly layout and paint, so `cargo run --release --example bench` (3000 nodes, one animating value) is the way to watch it: text sizes are cached and offscreen commands skipped, which took a frame of that scene from about 11 ms to under 3 ms, and skippable rows cut composing by half. Blocks marked `skippable` (or `#[composable(skippable)]`) are skipped when their arguments are equal and nothing they read has changed, but the parent that calls them still runs. There is no snapshot state, so `derived_state_of` only gives a result the type of state.
- `lazy_column`, `lazy_row` and `lazy_vertical_grid` need items of one fixed size; `lazy_list` takes any heights but estimates the ones it has not laid out yet, so its scrollbar thumb can change length while scrolling far into unmeasured items. It is vertical only.
- Scrollbars are drawn over the content at the end of the scrolling axis, and the strip along that edge takes the pointer from what is under it while the bar can be seen. Horizontal scrolling areas get one too; there is no way to move the bar to the other side.
- Text areas wrap at their width (`basic_text_area_no_wrap` scrolls sideways instead). Line height is one value per node, grown only by a span with a larger size, and a field's caret and selection use the layout of its text at the width it was laid out at. Underline, strikethrough and span backgrounds are rectangles from the shaped layout, not glyph decoration. Glyph advances are not snapped to whole pixels, so text sits at subpixel positions. `TextLayout` offsets are UTF-8 bytes while `TextFieldValue` counts characters. Bidirectional text follows the Unicode algorithm through cosmic-text, and a caret at the join of two runs sits at the logical position, not at both visual edges. A `selection_container` numbers blocks in composition order, so a selection moves if blocks are inserted before it, and it only knows blocks composed in the current pass (a lazy list drops those scrolled away). `layout_text`, `font_families` and `register_font` need a renderer to exist; fonts registered earlier are queued and added when it is created.
- Rounded clips (`clip_rounded`) each cost a texture the size of the window and an extra pass, so many of them on one screen (a message bubble per row, say) are slower than a rectangular `clip`; pointer input is still cut to the rectangle. `Modifier::alpha` fades each thing on its own; use `graphics_layer` for group-correct fading, rotation and scale, which draws the content to a texture the size of the window. Pointer input in a rotated or scaled area still goes where the area was laid out. Raised layers (`z_index`) are cut to the clips around where they were declared; use `z_index_unclipped` for popups, menus and dialogs that must leave a clipped or scrolling parent (`floating` and `dialog` do).
- Input is the mouse buttons (primary with double and triple click counts, plus secondary, middle, back, forward and others through `on_pointer_button`), a touch contact treated as the primary button, long presses, hover, the wheel, the keyboard (logical and physical keys, repeats and releases) and input-method text. Drag-and-drop, long presses and click counts are primary-button only, and a secondary or middle press does not move focus. Key bubbling follows layout containment, not the focus tree, so an `on_key_event` must be outside the `focusable` it wants to hear from. Only the primary button's presses carry a click count beyond one. Input-method composition is shown only in text fields: the caret rectangle is the caret's line, preedit styling (several segments, highlighted clause) is a single underline, and the platform's `ime_purpose` and surrounding-text API are not used. There are no key-chord sequences, global (outside the window) shortcuts, gamepads or pen pressure. Pinch gestures, files dropped from other applications, and dragging content from a `drag_source` to a `drop_target` inside the application are supported. There is no multi-finger input beyond pinch, and nothing can be dragged out of the window.
- Accessibility describes a flat list of areas with their role, label, value and state, and supports click and focus requests.
- Right-to-left layout is supported by providing `LOCAL_LAYOUT_DIRECTION`: rows, stacks, flows and start-aligned columns mirror, text starts on its own side, and `padding_directional` swaps. Scroll directions, sliders and other widgets do not mirror, and text is not localized.
