//! Transient hints: tooltips and snackbars.

use rustle_foundation::{
    Alignment, Arrangement, BoxAlignment, LOCAL_TEXT_STYLE, Modifier, floating, row_arranged,
    stack_with, text,
};
use rustle_graphics::Color;
use rustle_runtime::remember;

use crate::text_button;
use crate::theme::theme;

/// Emits `content` and, while the pointer is over it, a small label below it.
#[track_caller]
pub fn tooltip(label: &str, content: impl FnOnce()) {
    let theme = theme();
    let hovered = remember(|| false);
    let hover = hovered.clone();
    stack_with(
        Modifier::new().on_hover(move |over| hover.set(over)),
        || {
            content();
            if hovered.get() {
                floating(Modifier::new().align(BoxAlignment::BottomStart), || {
                    stack_with(
                        Modifier::new()
                            .offset(0.0, 6.0)
                            .shadow(4.0, 6.0)
                            .background_rounded(Color::rgb(50, 52, 66), 6.0)
                            .padding_xy(10.0, 6.0),
                        || {
                            let style = LOCAL_TEXT_STYLE
                                .current()
                                .color(Color::rgb(240, 240, 245))
                                .font_size(14.0)
                                .max_lines(1);
                            let _ = theme;
                            rustle_foundation::text_styled(
                                Modifier::new(),
                                label.to_string(),
                                style,
                            );
                        },
                    );
                });
            }
        },
    );
}

/// Emits a message bar, optionally with one action labelled `action_label`. Put it in a stack, aligned
/// to the bottom, over the content it should cover.
#[track_caller]
pub fn snackbar(message: &str, action_label: Option<&str>, on_action: impl Fn() + 'static) {
    row_arranged(
        Modifier::new()
            .shadow(6.0, 8.0)
            .background_rounded(Color::rgb(50, 52, 66), 8.0)
            .padding_each(16.0, 10.0, 8.0, 10.0),
        Arrangement::SpacedBy(16.0),
        Alignment::Center,
        || {
            let style = LOCAL_TEXT_STYLE.current().color(Color::rgb(240, 240, 245));
            LOCAL_TEXT_STYLE.provide(style, || text(message.to_string()));
            if let Some(label) = action_label {
                text_button(label, on_action);
            }
        },
    );
}
