//! Two native windows from one composition, each with a custom title bar.
//!
//! The first window has no system decorations: its title bar is drawn here, moves the window
//! when pressed, maximizes it on a double click, and has minimize, maximize and close
//! buttons. A button opens a second window with `window`, which closes with its own bar or
//! when the first button is used again. Run it with `cargo run --example multi_window`.

use rustle::prelude::*;
use rustle::{ResizeEdge, WindowHandle, current_window, run_with};

const BAR: Color = Color::rgba(32, 34, 40, 255);
const BODY: Color = Color::rgba(22, 24, 28, 255);

/// A title bar that drags the window, and has the three window buttons.
#[composable]
fn title_bar(title: &str) {
    let window: WindowHandle = current_window();
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .height(36.0)
            .background(BAR)
            .on_press(move |press| {
                if press.click_count == 2 {
                    window.toggle_maximize();
                } else {
                    window.drag_window();
                }
            }),
        Arrangement::SpaceBetween,
        Alignment::Center,
        || {
            text_with(Modifier::new().padding_xy(12.0, 0.0), title);
            row(|| {
                bar_button("-", move || window.minimize());
                bar_button("+", move || window.toggle_maximize());
                bar_button("x", move || window.close());
            });
        },
    );
}

#[composable]
fn bar_button(label: &str, on_click: impl Fn() + 'static) {
    text_with(
        Modifier::new()
            .width(44.0)
            .padding_xy(16.0, 8.0)
            .cursor(CursorIcon::Pointer)
            .clickable(on_click),
        label,
    );
}

/// A thin strip along the bottom edge that resizes the undecorated window.
#[composable]
fn bottom_grip() {
    let window = current_window();
    spacer(
        Modifier::new()
            .fill_max_width()
            .height(5.0)
            .cursor(ResizeEdge::South.cursor())
            .on_press(move |_| window.drag_resize_window(ResizeEdge::South)),
    );
}

#[composable]
fn app() {
    let second_open = remember(|| false);
    let clicks = remember(|| 0);
    column_with(Modifier::new().fill_max_size().background(BODY), || {
        title_bar("Main window");
        column_with(Modifier::new().weight(1.0).padding(16.0), || {
            text(format!("Clicked {} times", clicks.get()));
            button("Count", {
                let clicks = clicks.clone();
                move || clicks.update(|n| *n += 1)
            });
            button(
                if second_open.get() {
                    "Close second window"
                } else {
                    "Open second window"
                },
                {
                    let second_open = second_open.clone();
                    move || second_open.update(|open| *open = !*open)
                },
            );
        });
        bottom_grip();
    });

    // A window exists while this call is part of the composition. It shares `clicks`
    // with the main window, and has its own size, pointer and focus.
    if second_open.get() {
        window(
            WindowOptions::new("Second window")
                .with_size(420.0, 260.0)
                .with_decorations(false)
                .with_app_id("rustle-multi-window"),
            {
                let second_open = second_open.clone();
                move || second_open.set(false)
            },
            || {
                column_with(Modifier::new().fill_max_size().background(BODY), || {
                    title_bar("Second window");
                    text_with(
                        Modifier::new().padding(16.0),
                        format!("The main window was clicked {} times", clicks.get()),
                    );
                    bottom_grip();
                });
            },
        );
    }
}

fn main() {
    run_with(
        WindowOptions::new("Rustle windows")
            .with_size(640.0, 400.0)
            .with_decorations(false)
            .with_app_id("rustle-multi-window"),
        app,
    );
}
