//! Buttons, badges and avatars.

use rustle::prelude::*;

use crate::extended::extended_colors;

use super::{kbd, wrap};
use crate::ctx::Ctx;
use crate::ui::{icon_action, panel};

#[composable]
pub(super) fn buttons(ctx: &Ctx) {
    let loading = remember(|| false);
    let group = remember(|| 1_usize);
    let ctx = ctx.clone();
    panel("Button", "Variants, icons, loading and groups.", || {
        wrap(|| {
            for (label, style) in [
                ("Default", ButtonStyle::Default),
                ("Secondary", ButtonStyle::Secondary),
                ("Outline", ButtonStyle::Outline),
                ("Ghost", ButtonStyle::Ghost),
                ("Destructive", ButtonStyle::Destructive),
            ] {
                key(label, || {
                    let ctx = ctx.clone();
                    styled_button(Modifier::new(), style, label, None, None, move || {
                        ctx.say(format!("{label} button"))
                    });
                });
            }
        });
        spacer(Modifier::new().height(10.0));
        wrap(|| {
            let c = ctx.clone();
            styled_button(
                Modifier::new(),
                ButtonStyle::Default,
                "Login with email",
                Some(phosphor::ENVELOPE_SIMPLE),
                None,
                move || c.say("Check your inbox"),
            );
            let c = ctx.clone();
            styled_button(
                Modifier::new(),
                ButtonStyle::Outline,
                "Continue",
                None,
                Some(phosphor::ARROW_RIGHT),
                move || c.say("Continuing"),
            );
            for (index, glyph) in [phosphor::PLUS, phosphor::MAGNIFYING_GLASS, phosphor::GEAR]
                .into_iter()
                .enumerate()
            {
                key(index, || {
                    let c = ctx.clone();
                    icon_action(glyph, "Icon button", move || c.say("Icon button"));
                });
            }
        });
        spacer(Modifier::new().height(10.0));
        wrap(|| {
            // Loading: a click starts a spinner that stops by itself.
            let busy = loading.clone();
            launched_effect(loading.get(), move |scope| async move {
                if busy.get() {
                    scope.delay(std::time::Duration::from_millis(1800)).await;
                    busy.set(false);
                }
            });
            let start = loading.clone();
            row_arranged(
                Modifier::new().width(150.0).height(34.0),
                Arrangement::Center,
                Alignment::Center,
                || {
                    if loading.get() {
                        row_arranged(
                            Modifier::new()
                                .fill_max_size()
                                .background_rounded(color_scheme().surface_variant, 8.0),
                            Arrangement::Center,
                            Alignment::Center,
                            || {
                                circular_progress_indicator_indeterminate(16.0);
                                spacer(Modifier::new().width(8.0));
                                muted("Please wait");
                            },
                        );
                    } else {
                        styled_button(
                            Modifier::new().fill_max_size(),
                            ButtonStyle::Default,
                            "Save changes",
                            None,
                            None,
                            move || start.set(true),
                        );
                    }
                },
            );
            segmented_control(group.get(), &["Day", "Week", "Month"], {
                let group = group.clone();
                move |index| group.set(index)
            });
        });
    });
}

// --- Badge, Avatar, Kbd, Separator -------------------------------------------

#[composable]
pub(super) fn badges() {
    let theme = color_scheme();
    panel("Badge", "Status, counts and keyboard hints.", || {
        wrap(|| {
            pill("Badge", BadgeStyle::Default);
            pill("Secondary", BadgeStyle::Secondary);
            pill("Outline", BadgeStyle::Outline);
            row_arranged(
                Modifier::new()
                    .border(1.0, theme.outline, 100.0)
                    .padding_xy(8.0, 2.0),
                Arrangement::SpacedBy(6.0),
                Alignment::Center,
                || {
                    spacer(
                        Modifier::new()
                            .size(7.0, 7.0)
                            .background_rounded(extended_colors().success, 4.0),
                    );
                    muted_sized("Online", 12.0);
                },
            );
            row_arranged(
                Modifier::new()
                    .border(1.0, theme.outline, 100.0)
                    .padding_xy(8.0, 2.0),
                Arrangement::SpacedBy(6.0),
                Alignment::Center,
                || {
                    icon(phosphor::CHECK, 12.0, extended_colors().success);
                    muted_sized("Verified", 12.0);
                },
            );
            badge(8);
        });
        spacer(Modifier::new().height(14.0));
        row_arranged(
            Modifier::new(),
            Arrangement::SpacedBy(4.0),
            Alignment::Center,
            || {
                muted_sized("Open the palette with", 13.0);
                kbd("Ctrl");
                kbd("K");
            },
        );
    });
}

#[composable]
pub(super) fn avatars() {
    let theme = color_scheme();
    panel("Avatar", "Single, and stacked as a group.", || {
        row_arranged(
            Modifier::new(),
            Arrangement::SpacedBy(16.0),
            Alignment::Center,
            || {
                avatar("SH", 40.0);
                row_arranged(
                    Modifier::new(),
                    Arrangement::Start,
                    Alignment::Center,
                    || {
                        for (index, initials) in ["EL", "JT", "SD", "+3"].into_iter().enumerate() {
                            key(index, || {
                                stack_aligned(
                                    Modifier::new().offset(index as f32 * -10.0, 0.0).border(
                                        2.0,
                                        theme.surface,
                                        20.0,
                                    ),
                                    BoxAlignment::Center,
                                    || avatar(initials, 36.0),
                                );
                            });
                        }
                    },
                );
            },
        );
    });
}
