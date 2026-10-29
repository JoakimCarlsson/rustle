//! Modal surfaces: dialogs, alert dialogs and bottom sheets.

use std::rc::Rc;

use rustle_animation::{animate_float_as_state, tween};
use rustle_foundation::{
    Alignment, Arrangement, BoxAlignment, LOCAL_TEXT_STYLE, Modifier, Role, Semantics, column_with,
    floating, row_arranged, spacer, stack_aligned, stack_with, text,
};
use rustle_graphics::Color;
use rustle_runtime::remember;

use crate::card;
use crate::text_button;
use crate::theme::{theme, with_text_color};

/// Emits a modal dialog: a dimmed layer over the whole area with `content` in
/// a card at its center. Clicking the dimmed area calls `on_dismiss`.
///
/// It fills the space it is given, so emit it last in a stack that fills the
/// window, where it covers what was emitted before it.
#[track_caller]
pub fn dialog(on_dismiss: impl Fn() + 'static, content: impl FnOnce()) {
    stack_aligned(
        Modifier::new()
            .fill_max_size()
            .semantics(Semantics::new(Role::Dialog))
            .clickable_plain(on_dismiss)
            .background(Color::rgba(0, 0, 0, 140)),
        BoxAlignment::Center,
        || {
            // The card takes clicks so they do not reach the dimmed area behind it.
            card(Modifier::new().clickable_plain(|| {}), || {
                stack_with(Modifier::new().padding(24.0), content);
            });
        },
    );
}

/// Emits a dialog with a `title`, a `message` and two buttons: "Cancel", which
/// calls `on_dismiss`, and `confirm_label`, which calls `on_confirm`. Clicking
/// outside it also calls `on_dismiss`. Emit it last in a stack that fills the window.
#[track_caller]
pub fn alert_dialog(
    title: &str,
    message: &str,
    confirm_label: &str,
    on_confirm: impl Fn() + 'static,
    on_dismiss: impl Fn() + 'static,
) {
    let on_dismiss = Rc::new(on_dismiss);
    let cancel = Rc::clone(&on_dismiss);
    dialog(
        move || on_dismiss(),
        || {
            column_with(Modifier::new().width_in(260.0, 380.0), || {
                let heading = LOCAL_TEXT_STYLE.current().font_size(22.0).bold();
                rustle_foundation::text_styled(Modifier::new(), title.to_string(), heading);
                spacer(Modifier::new().height(8.0));
                text(message.to_string());
                spacer(Modifier::new().height(20.0));
                row_arranged(
                    Modifier::new().fill_max_width(),
                    Arrangement::End,
                    Alignment::Center,
                    || {
                        text_button("Cancel", move || cancel());
                        text_button(confirm_label, on_confirm);
                    },
                );
            });
        },
    );
}

/// Emits a sheet that slides up from the bottom with `content` while `visible`,
/// over a dimmed layer that calls `on_dismiss` when clicked. Emit it last in a
/// stack that fills the window.
#[track_caller]
pub fn bottom_sheet(visible: bool, on_dismiss: impl Fn() + 'static, content: impl FnOnce()) {
    rustle_runtime::group(|| {
        let theme = theme();
        let progress = animate_float_as_state(if visible { 1.0 } else { 0.0 }, tween(260)).get();
        let height = remember(|| std::rc::Rc::new(std::cell::Cell::new(300.0_f32))).get();
        if !visible && progress < 0.001 {
            return;
        }
        let measured = std::rc::Rc::clone(&height);
        stack_aligned(
            Modifier::new().fill_max_size().z_index(1),
            BoxAlignment::BottomCenter,
            || {
                spacer(
                    Modifier::new()
                        .fill_max_size()
                        .alpha(progress)
                        .background(Color::rgba(0, 0, 0, 140))
                        .clickable_plain(on_dismiss),
                );
                stack_with(
                    Modifier::new()
                        .fill_max_width()
                        .offset(0.0, (1.0 - progress) * height.get())
                        .on_size_changed(move |size| measured.set(size.height))
                        .shadow(10.0, 16.0)
                        .background_rounded(theme.surface, 16.0)
                        // Takes clicks, so they do not dismiss the sheet.
                        .clickable_plain(|| {})
                        .padding(24.0),
                    || with_text_color(theme.on_surface, content),
                );
            },
        );
    });
}

/// Emits `content` in a layer above everything else, positioned where it is
/// emitted: its top-left corner is at the end of what was emitted before it.
/// Clicking outside `content` calls `on_dismiss`.
#[track_caller]
pub fn popup(on_dismiss: impl Fn() + 'static, content: impl FnOnce()) {
    floating(Modifier::new(), || {
        // An invisible layer, larger than any window, that catches outside clicks.
        spacer(
            Modifier::new()
                .size(20_000.0, 20_000.0)
                .offset(-10_000.0, -10_000.0)
                .clickable_plain(on_dismiss),
        );
        content();
    });
}
