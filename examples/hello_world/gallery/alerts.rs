//! Alert callouts.

use rustle::prelude::*;

use crate::ui::panel;

#[composable]
pub(super) fn callout(glyph: Icon, heading: &str, body: &str, destructive: bool) {
    let theme = color_scheme();
    let color = if destructive {
        theme.error
    } else {
        theme.on_surface
    };
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .border(
                1.0,
                if destructive {
                    theme.error
                } else {
                    theme.outline
                },
                10.0,
            )
            .padding(14.0),
        Arrangement::SpacedBy(12.0),
        Alignment::Start,
        || {
            icon(glyph, 16.0, color);
            column_with(Modifier::new().weight(1.0), || {
                text_styled(
                    Modifier::new(),
                    heading.to_string(),
                    LOCAL_TEXT_STYLE
                        .current()
                        .color(color)
                        .font_weight(FontWeight::MEDIUM),
                );
                muted_sized(body, 13.0);
            });
        },
    );
    spacer(Modifier::new().height(8.0));
}

#[composable]
pub(super) fn alerts() {
    panel("Alert", "Callouts for things worth reading.", || {
        callout(
            phosphor::PULSE,
            "Heads up!",
            "You can add components to your app from the command line.",
            false,
        );
        callout(
            phosphor::LOCK,
            "Your session has expired",
            "Please log in again to keep editing.",
            true,
        );
    });
}
