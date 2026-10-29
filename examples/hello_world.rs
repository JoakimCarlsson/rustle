use rustle::prelude::*;

#[composable]
fn app() {
    let count = remember(|| 0);

    column(|| {
        text("Hello, Rustle!");
        text("A tiny declarative UI.");
        text("Rendered with wgpu.");
        text(format!("Count: {}", count.get()));
    });
}

fn main() {
    rustle::run(app);
}
