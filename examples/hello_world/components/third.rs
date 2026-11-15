//! The third column: the transfer form.

use rustle::prelude::*;

use super::ACCOUNTS;
use crate::ctx::{Ctx, Overlay};
use crate::ui::{key_value, labelled, money, panel, parse_money};

pub(super) fn account_names(balances: [f64; 3]) -> Vec<String> {
    ACCOUNTS
        .iter()
        .zip(balances)
        .map(|((name, last), balance)| format!("{name} ({last}) \u{2014} {}", money(balance)))
        .collect()
}

#[composable]
pub(super) fn third_column(ctx: &Ctx) {
    let theme = color_scheme();
    let url = remember_text_field_state("https://example.com/ledger/connect");
    let refreshes = remember(|| 0u32);
    let amount = remember_text_field_state("1,200.00");
    let from = remember(|| 0usize);
    let to = remember(|| 1usize);
    let expanded = remember(|| None::<usize>);
    let balances = ctx.balances.get();
    let value = parse_money(&amount.text());

    card(Modifier::new().fill_max_width(), || {
        column_arranged(
            Modifier::new().fill_max_width().padding(24.0),
            Arrangement::Start,
            Alignment::Center,
            || {
                let data = format!("{}?r={}", url.text(), refreshes.get());
                qr_code(
                    220.0,
                    &data,
                    Color::rgb(9, 9, 11),
                    Color::rgb(255, 255, 255),
                );
                spacer(Modifier::new().height(16.0));
                text_field(Modifier::new().fill_max_width(), &url, "Link to encode");
                spacer(Modifier::new().height(10.0));
                let (more, ctx) = (refreshes.clone(), ctx.clone());
                styled_button(
                    Modifier::new().fill_max_width(),
                    ButtonStyle::Outline,
                    "Refresh code",
                    None,
                    None,
                    move || {
                        more.update(|n| *n += 1);
                        ctx.say("New code generated");
                    },
                );
                spacer(Modifier::new().height(16.0));
                title("Scan to connect your mobile device", 14.0);
                spacer(Modifier::new().height(6.0));
                muted_sized(
                    "Open the Ledger mobile app and scan this code to link your device.",
                    12.0,
                );
            },
        );
    });
    spacer(Modifier::new().height(16.0));
    panel(
        "Transfer Funds",
        "Move money between your connected accounts.",
        || {
            labelled("Amount to Transfer", || {
                text_field(Modifier::new().fill_max_width(), &amount, "0.00");
            });
            spacer(Modifier::new().height(12.0));
            labelled("From Account", || {
                let pick = from.clone();
                select(
                    Modifier::new().fill_max_width(),
                    account_names(balances),
                    from.get(),
                    move |i| pick.set(i),
                );
            });
            spacer(Modifier::new().height(12.0));
            labelled("To Account", || {
                let pick = to.clone();
                select(
                    Modifier::new().fill_max_width(),
                    account_names(balances),
                    to.get(),
                    move |i| pick.set(i),
                );
            });
            spacer(Modifier::new().height(14.0));
            column_with(
                Modifier::new()
                    .fill_max_width()
                    .background_rounded(theme.secondary_container, 10.0)
                    .border(1.0, theme.outline, 10.0)
                    .padding(14.0),
                || {
                    key_value("Estimated arrival", "Today, Apr 14", false);
                    key_value("Transaction fee", "$0.00", false);
                    separator();
                    key_value(
                        "Total amount",
                        &value.map_or("Invalid amount".into(), money),
                        true,
                    );
                },
            );
            spacer(Modifier::new().height(14.0));
            let (ctx, from, to) = (ctx.clone(), from.clone(), to.clone());
            styled_button(
                Modifier::new().fill_max_width(),
                ButtonStyle::Default,
                "Confirm Transfer",
                None,
                None,
                move || {
                    let (from, to) = (from.get(), to.get());
                    let balances = ctx.balances.get();
                    match value {
                        None | Some(0.0) => ctx.say("Enter an amount to transfer"),
                        _ if from == to => ctx.say("Pick two different accounts"),
                        Some(v) if v > balances[from] => {
                            ctx.say("Not enough funds in that account")
                        }
                        Some(v) => {
                            ctx.transfer.set((v, from, to));
                            ctx.open(Overlay::Transfer);
                        }
                    }
                },
            );
        },
    );
    panel("", "", || {
        row_arranged(
            Modifier::new(),
            Arrangement::SpacedBy(8.0),
            Alignment::Center,
            || {
                let home = ctx.clone();
                stack_with(
                    Modifier::new()
                        .semantics(Semantics::new(Role::Button).label("Home".to_string()))
                        .clickable(move || home.go(0)),
                    || muted_sized("Home", 13.0),
                );
                icon(phosphor::CARET_RIGHT, 12.0, theme.on_surface_variant);
                let trail = [("Billing", 8usize), ("Cards", 2), ("Statements", 6)]
                    .map(|(label, page)| {
                        let ctx = ctx.clone();
                        MenuItem::new(label, move || ctx.go(page))
                    })
                    .to_vec();
                menu(
                    |open| {
                        stack_with(
                            Modifier::new()
                                .semantics(Semantics::new(Role::Button).label("More".to_string()))
                                .clickable(move || open()),
                            || icon(phosphor::DOTS_THREE, 14.0, theme.on_surface_variant),
                        )
                    },
                    trail,
                );
                icon(phosphor::CARET_RIGHT, 12.0, theme.on_surface_variant);
                text("Payments".to_string());
            },
        );
        spacer(Modifier::new().height(14.0));
        for (index, (glyph, heading, detail)) in [
            (
                phosphor::TREND_UP,
                "Change transfer limit",
                "Adjust how much you can send from your balance.",
            ),
            (
                phosphor::CALENDAR_BLANK,
                "Scheduled transfers",
                "Set up a transfer to send at a later date.",
            ),
            (
                phosphor::CREDIT_CARD,
                "Recurring card payments",
                "Manage subscriptions on this card.",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            key(heading, || {
                let open = expanded.get() == Some(index);
                let toggle = expanded.clone();
                column_with(
                    Modifier::new()
                        .fill_max_width()
                        .semantics(
                            Semantics::new(Role::Button)
                                .label(heading.to_string())
                                .merging_descendants(),
                        )
                        .clickable(move || toggle.set(if open { None } else { Some(index) }))
                        .background_rounded(theme.secondary_container, 10.0)
                        .padding(14.0),
                    || {
                        row_arranged(
                            Modifier::new().fill_max_width(),
                            Arrangement::SpaceBetween,
                            Alignment::Center,
                            || {
                                row_arranged(
                                    Modifier::new(),
                                    Arrangement::SpacedBy(12.0),
                                    Alignment::Center,
                                    || {
                                        icon(glyph, 16.0, theme.on_surface_variant);
                                        column_with(Modifier::new().width_in(120.0, 260.0), || {
                                            title(heading, 13.0);
                                            muted_sized(detail, 12.0);
                                        });
                                    },
                                );
                                icon(
                                    if open {
                                        phosphor::CARET_DOWN
                                    } else {
                                        phosphor::CARET_RIGHT
                                    },
                                    14.0,
                                    theme.on_surface_variant,
                                );
                            },
                        );
                        if open {
                            spacer(Modifier::new().height(10.0));
                            let go = ctx.clone();
                            styled_button(
                                Modifier::new(),
                                ButtonStyle::Outline,
                                "Manage",
                                None,
                                Some(phosphor::ARROW_RIGHT),
                                move || go.say(format!("Managing: {heading}")),
                            );
                        }
                    },
                );
                spacer(Modifier::new().height(8.0));
            });
        }
    });
}
