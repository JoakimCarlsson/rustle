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
