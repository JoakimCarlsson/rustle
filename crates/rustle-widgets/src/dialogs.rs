//! Modal surfaces: dialogs, alert dialogs and bottom sheets.

use std::rc::Rc;

use rustle_animation::{animate_float_as_state, tween};
use rustle_foundation::{
    Alignment, Arrangement, BoxAlignment, Key, Modifier, PointerButton, Role, Semantics,
    column_with, floating, row_arranged, spacer, stack_aligned, stack_with, text,
};
use rustle_graphics::Color;
use rustle_runtime::remember;

use crate::card;
use crate::text_button;
use crate::theme::{color_scheme, provide_content_color, text_style_of, typography};

/// Emits a modal dialog: a dimmed layer over the whole area with `content` in
/// a card at its center. Clicking the dimmed area calls `on_dismiss`.
///
/// It fills the space it is given, so emit it last in a stack that fills the
/// window, where it covers what was emitted before it.
#[track_caller]
pub fn dialog(on_dismiss: impl Fn() + 'static, content: impl FnOnce()) {
    dialog_semantics(Semantics::new(Role::Dialog), on_dismiss, content);
}

/// How a dialog behaves: Compose's `DialogProperties`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DialogProperties {
    /// Whether Escape, the desktop's back press, asks to dismiss the dialog, when it reaches
    /// the dialog: from what has the keyboard inside it, or with nothing focused.
    pub dismiss_on_back_press: bool,
    /// Whether a press on the dimmed area outside the dialog's content asks to dismiss it.
    pub dismiss_on_click_outside: bool,
}

impl Default for DialogProperties {
    /// Compose's defaults: dismissed by Escape and by a press outside.
    fn default() -> Self {
        Self {
            dismiss_on_back_press: true,
            dismiss_on_click_outside: true,
        }
    }
}

/// The color of the layer that dims what lies behind a dialog.
const SCRIM: Color = Color::rgba(0, 0, 0, 140);

/// Emits a modal dialog as Compose's `Dialog(onDismissRequest, properties, content)`: a
/// dimmed layer over the whole area with `content` at its center, drawn as it is given, with
/// no container of its own. Nothing beneath the dimmed layer takes pointer presses while it
/// is shown. As `properties` say, a press on the dimmed layer and Escape call
/// `on_dismiss_request`; hide the dialog there by not emitting it any more.
///
/// It fills the space it is given, so emit it last in a stack that fills the window, where
/// it covers what was emitted before it.
#[track_caller]
pub fn dialog_with(
    on_dismiss_request: impl Fn() + 'static,
    properties: DialogProperties,
    content: impl FnOnce(),
) {
    let on_dismiss_request = Rc::new(on_dismiss_request);
    let mut scrim = Modifier::new()
        .fill_max_size()
        .semantics(Semantics::new(Role::Dialog));
    for button in [
        PointerButton::Primary,
        PointerButton::Secondary,
        PointerButton::Middle,
    ] {
        scrim = scrim.on_pointer_button(button, |_| {});
    }
    if properties.dismiss_on_back_press {
        let dismiss = Rc::clone(&on_dismiss_request);
        scrim = scrim.on_key_event(move |event| {
            let escape = event.pressed && event.key == Key::Escape;
            if escape {
                dismiss();
            }
            escape
        });
    }
    let scrim = if properties.dismiss_on_click_outside {
        scrim.clickable_plain(move || on_dismiss_request())
    } else {
        scrim.clickable_plain(|| {})
    };
    stack_aligned(scrim.background(SCRIM), BoxAlignment::Center, || {
        stack_with(Modifier::new().clickable_plain(|| {}), content);
    });
}

/// Emits a dialog described to assistive technology by `semantics`.
#[track_caller]
fn dialog_semantics(semantics: Semantics, on_dismiss: impl Fn() + 'static, content: impl FnOnce()) {
    stack_aligned(
        Modifier::new()
            .fill_max_size()
            .semantics(semantics)
            .clickable_plain(on_dismiss)
            .background(SCRIM),
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
    dialog_semantics(
        Semantics::new(Role::Dialog).label(title.to_string()),
        move || on_dismiss(),
        || {
            column_with(Modifier::new().width_in(260.0, 380.0), || {
                let heading = text_style_of(typography().title_large);
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
        let theme = color_scheme();
        let progress = animate_float_as_state(if visible { 1.0 } else { 0.0 }, tween(260)).get();
        let height = remember(|| std::rc::Rc::new(std::cell::Cell::new(300.0_f32))).get();
        if !visible && progress < 0.001 {
            return;
        }
        let measured = std::rc::Rc::clone(&height);
        stack_aligned(
            Modifier::new().fill_max_size().z_index_unclipped(1),
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
                    || provide_content_color(theme.on_surface, content),
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

#[cfg(test)]
mod tests;
