//! A catalogue in the manner of shadcn/ui's component docs: one small card per
//! component, each with its variants, each one working.

mod alerts;
mod buttons;
mod cards;
mod forms;
mod gestures;
mod layouts;
mod navigation;
mod overlays;

use rustle::prelude::*;

use crate::ctx::Ctx;
use crate::pages::page_frame;
use alerts::alerts;
use buttons::{avatars, badges, buttons};
use cards::{login_card, tables};
use forms::{choices, inputs, otp, selects};
use gestures::gestures;
use layouts::layouts;
use navigation::navigation;
use overlays::{calendars, collapsible, hover_card, overlays};

/// A row of things that wrap when there is no room.
#[composable]
fn wrap(content: impl FnOnce()) {
    flow_row(Modifier::new().fill_max_width(), 8.0, 8.0, content);
}

#[composable]
fn kbd(label: &str) {
    let theme = theme();
    stack_aligned(
        Modifier::new()
            .height(22.0)
            .width_in(22.0, 120.0)
            .background_rounded(theme.accent, 5.0)
            .border(1.0, theme.outline, 5.0)
            .padding_xy(6.0, 0.0),
        BoxAlignment::Center,
        || {
            text_styled(
                Modifier::new(),
                label.to_string(),
                LOCAL_TEXT_STYLE
                    .current()
                    .color(theme.on_surface_muted)
                    .font_size(11.0)
                    .font_family(FontFamily::Monospace),
            );
        },
    );
}

#[composable]
pub fn gallery_page(ctx: &Ctx) {
    page_frame(
        "Gallery",
        "Every component in one place, in the manner of the shadcn docs.",
        || {
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpacedBy(16.0),
                Alignment::Start,
                || {
                    column_with(Modifier::new().weight(1.0), || {
                        buttons(ctx);
                        inputs(ctx);
                        choices();
                        login_card(ctx);
                        calendars();
                        layouts();
                    });
                    column_with(Modifier::new().weight(1.0), || {
                        gestures();
                        badges();
                        avatars();
                        alerts();
                        navigation();
                        selects(ctx);
                        otp();
                        tables();
                        collapsible();
                        hover_card();
                        overlays(ctx);
                    });
                },
            );
        },
    );
}
