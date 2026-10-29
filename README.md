# Rustle

A minimal declarative UI framework in Rust, inspired by Jetpack Compose. This is the smallest working foundation: composition, a vertical column layout, and a `wgpu` renderer.

```rust
use rustle::prelude::*;

#[composable]
fn app() {
    column(|| {
        text("Hello, Rustle!");
        text("A tiny declarative UI.");
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

See [docs/architecture.md](docs/architecture.md) for the crates and what belongs in each.

Pipeline: a `Composition` runs the root into a `Node` tree, `layout` turns it into `DrawCommand`s, and `Renderer::render` draws them. State changes mark the composition dirty and wake the event loop, which composes again.

## Not implemented yet

State (`remember`), recomposition, input, modifiers, styling, other layouts, images, scrolling, animation, accessibility.
