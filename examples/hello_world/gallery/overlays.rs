//! Collapsibles, hover cards, overlays and calendars.

use rustle::prelude::*;

use super::wrap;
use crate::ctx::{Ctx, Overlay};
use crate::ui::panel;

#[composable]
pub(super) fn collapsible() {
    let theme = theme();
    let open = remember(|| false);
    let turn = animate_float_as_state(if open.get() { 180.0 } else { 0.0 }, tween(200)).get();
    panel("Collapsible", "Show a little, or all of it.", || {
        let toggle = open.clone();
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceBetween,
            Alignment::Center,
            || {
                title("3 repositories starred", 14.0);
                stack_aligned(
                    Modifier::new()
                        .size(30.0, 30.0)
                        .semantics(
                            Semantics::new(Role::Button)
                                .label("Toggle".to_string())
                                .toggled(open.get()),
                        )
                        .clickable(move || toggle.update(|v| *v = !*v)),
                    BoxAlignment::Center,
                    || {
                        stack_aligned(
                            Modifier::new().size(16.0, 16.0).rotate(turn),
                            BoxAlignment::Center,
                            || icon(phosphor::CARET_DOWN, 16.0, theme.on_surface_muted),
                        );
                    },
                );
            },
        );
        spacer(Modifier::new().height(8.0));
        for (name, always) in [
            ("rustle/ui", true),
            ("rustle/wgpu", false),
            ("rustle/charts", false),
        ] {
            if always {
                key(name, || repo_row(name));
            } else {
                key(name, || animated_visibility(open.get(), || repo_row(name)));
            }
        }
    });
}

#[composable]
pub(super) fn repo_row(name: &str) {
    let theme = theme();
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .border(1.0, theme.outline, 8.0)
            .padding_xy(12.0, 8.0),
        Arrangement::Start,
        Alignment::Center,
        || {
            text_styled(
                Modifier::new(),
                name.to_string(),
                LOCAL_TEXT_STYLE
                    .current()
                    .font_family(FontFamily::Monospace)
                    .font_size(13.0),
            );
        },
    );
    spacer(Modifier::new().height(6.0));
}

#[composable]
pub(super) fn hover_card() {
    let theme = theme();
    let hovered = remember(|| false);
    let hover = hovered.clone();
    let shown = animate_float_as_state(if hovered.get() { 1.0 } else { 0.0 }, tween(160)).get();
    panel(
        "Hover card",
        "Preview a profile without leaving the page.",
        || {
            stack_with(
                Modifier::new().on_hover(move |over| hover.set(over)),
                || {
                    row_arranged(
                        Modifier::new().padding_xy(0.0, 2.0),
                        Arrangement::SpacedBy(8.0),
                        Alignment::Center,
                        || {
                            avatar("SH", 24.0);
                            text_styled(
                                Modifier::new(),
                                "@shadcn".to_string(),
                                LOCAL_TEXT_STYLE.current().font_weight(FontWeight::MEDIUM),
                            );
                        },
                    );
                    if shown > 0.01 {
                        floating(Modifier::new().align(BoxAlignment::BottomStart), || {
                            column_with(
                                Modifier::new()
                                    .offset(0.0, 8.0 - shown * 2.0)
                                    .alpha(shown)
                                    .width(260.0)
                                    .shadow(10.0, 10.0)
                                    .background_rounded(theme.surface, 10.0)
                                    .border(1.0, theme.outline, 10.0)
                                    .padding(14.0),
                                || {
                                    row_arranged(
                                        Modifier::new(),
                                        Arrangement::SpacedBy(12.0),
                                        Alignment::Center,
                                        || {
                                            avatar("SH", 40.0);
                                            column(|| {
                                                title("@shadcn", 14.0);
                                                muted_sized("Joined December 2021", 12.0);
                                            });
                                        },
                                    );
                                    spacer(Modifier::new().height(10.0));
                                    muted_sized(
                                        "Beautifully designed components, built with care.",
                                        13.0,
                                    );
                                },
                            );
                        });
                    }
                },
            );
            spacer(Modifier::new().height(36.0));
        },
    );
}

#[composable]
pub(super) fn overlays(ctx: &Ctx) {
    panel(
        "Dialog, sheet and toast",
        "Layers that open over the page.",
        || {
            wrap(|| {
                for (label, overlay) in [
                    ("Dialog", Overlay::Invite),
                    ("Alert dialog", Overlay::Alert),
                    ("Sheet", Overlay::NewDocument),
                    ("Command", Overlay::Search),
                ] {
                    key(label, || {
                        let ctx = ctx.clone();
                        styled_button(
                            Modifier::new(),
                            ButtonStyle::Outline,
                            label,
                            None,
                            None,
                            move || ctx.open(overlay.clone()),
                        );
                    });
                }
            });
            spacer(Modifier::new().height(10.0));
            wrap(|| {
                for message in ["Event created", "Changes saved", "Link copied"] {
                    key(message, || {
                        let ctx = ctx.clone();
                        styled_button(
                            Modifier::new(),
                            ButtonStyle::Secondary,
                            message,
                            None,
                            None,
                            move || ctx.say(message),
                        );
                    });
                }
            });
        },
    );
}

#[composable]
pub(super) fn calendars() {
    let month = remember(|| (2025_i32, 6_u32));
    let picked = remember(|| 12_usize);
    let (year, number) = month.get();
    let names = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    panel(
        "Calendar",
        "Pick a date, or page through the months.",
        || {
            let (pick, step) = (picked.clone(), month.clone());
            let days = days_in_month(year, number);
            calendar(
                &format!("{} {year}", names[number as usize - 1]),
                first_weekday(year, number),
                days,
                Some((picked.get().min(days), picked.get().min(days))),
                move |direction| {
                    step.update(|(y, m)| {
                        let index = (*y * 12 + *m as i32 - 1) + direction;
                        *y = index.div_euclid(12);
                        *m = index.rem_euclid(12) as u32 + 1;
                    })
                },
                move |day| pick.set(day),
            );
        },
    );
}
