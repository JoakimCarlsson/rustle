# Rustle

A minimal, Compose-inspired declarative UI framework in Rust. This is the smallest working foundation: composition, a vertical column layout, and a `wgpu` renderer.

```rust
use rustle::prelude::*;

#[composable]
fn app() {
    column(|| {
        text("Hello, Rustle!");
        text("A tiny Compose-inspired UI.");
        text("Rendered with wgpu.");
    });
}

fn main() {
    rustle::run(app);
}
```

Run it with:

```
cargo run --example hello_world
```

## Architecture

| Crate | Role |
| --- | --- |
| `rustle-core` | Composition (thread-local scope stack), `Node` tree, layout, `DrawCommand` display list. No graphics dependencies. |
| `rustle-macros` | `#[composable]` (currently a pass-through). |
| `rustle-wgpu` | wgpu + glyphon renderer. Consumes only `DrawCommand`s. |
| `rustle` | Public API, prelude, and the winit-based `run`. |

Pipeline: `compose` builds a `Node` tree, `layout` turns it into `DrawCommand`s, and `Renderer::render` draws them.

## Not implemented yet

State (`remember`), recomposition, input, modifiers, styling, other layouts, images, scrolling, animation, accessibility.
