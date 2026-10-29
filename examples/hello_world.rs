use rustle::prelude::*;

#[composable]
fn app() {
    let count = remember(|| 0);

    column_with(Modifier::new().padding(24.0), || {
        text("Hello, Rustle!");
        text("A tiny declarative UI.");
        text("Rendered with wgpu.");
        spacer(Modifier::new().height(16.0));

        row_with(
            Modifier::new()
                .background_rounded(Color::rgb(48, 52, 74), 12.0)
                .padding_xy(20.0, 12.0),
            || {
                text("Count: ");
                text(count.get().to_string());
            },
        );
        spacer(Modifier::new().height(16.0));

        button("Increment", move || count.update(|n| *n += 1));
    });
}

fn main() {
    rustle::run(app);
}
