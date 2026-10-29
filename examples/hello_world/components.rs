//! A catalogue of controls, each doing something real.

use rustle::prelude::*;

use crate::ctx::{Ctx, Overlay};
use crate::ui::{chart_style, key_value, labelled, meter, money, panel, parse_money, tinted};

pub const ACCOUNTS: [(&str, &str); 3] = [
    ("Main Checking", "-8402"),
    ("High Yield Savings", "-1192"),
    ("Brokerage", "-3307"),
];

type Entry = (Icon, &'static str);

#[composable]
fn nav_list(heading: &str, entries: &[Entry], filter: &str, toast: &Ctx) {
    let theme = theme();
    let selected = remember(|| None::<usize>);
    card(Modifier::new().weight(1.0), || {
        column_with(Modifier::new().fill_max_width().padding(12.0), || {
            stack_with(Modifier::new().padding_xy(8.0, 4.0), || {
                muted_sized(heading, 12.0)
            });
            let needle = filter.to_lowercase();
            for (index, (glyph, label)) in entries.iter().enumerate() {
                let label: &'static str = label;
                if !label.to_lowercase().contains(&needle) {
                    continue;
                }
                key(index, || {
                    let active = selected.get() == Some(index);
                    let (pick, ctx) = (selected.clone(), toast.clone());
                    let color = if active {
                        theme.on_surface
                    } else {
                        theme.on_surface_muted
                    };
                    row_arranged(
                        Modifier::new()
                            .fill_max_width()
                            .semantics(
                                Semantics::new(Role::Button)
                                    .label(label.to_string())
                                    .merging_descendants(),
                            )
                            .clickable(move || {
                                pick.set(Some(index));
                                ctx.say(format!("Opened {label}"));
                            })
                            .background_rounded(
                                if active {
                                    theme.accent
                                } else {
                                    Color::TRANSPARENT
                                },
                                6.0,
                            )
                            .padding_xy(8.0, 7.0),
                        Arrangement::SpacedBy(10.0),
                        Alignment::Center,
                        || {
                            icon(*glyph, 15.0, color);
                            text_styled(
                                Modifier::new(),
                                label.to_string(),
                                LOCAL_TEXT_STYLE.current().color(color),
                            );
                        },
                    );
                });
            }
        });
    });
}

#[composable]
fn target_card(ctx: &Ctx, label: &str, goal: f64, saved: State<f64>) {
    let theme = theme();
    let percent = (saved.get() / goal).clamp(0.0, 1.0) as f32;
    column_with(
        Modifier::new()
            .fill_max_width()
            .background_rounded(theme.secondary, 10.0)
            .border(1.0, theme.outline, 10.0)
            .padding(16.0),
        || {
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpaceBetween,
                Alignment::Center,
                || {
                    column(|| {
                        muted_sized(label, 11.0);
                        spacer(Modifier::new().height(4.0));
                        title(&money(goal), 26.0);
                    });
                    row_arranged(
                        Modifier::new(),
                        Arrangement::SpacedBy(6.0),
                        Alignment::Center,
                        || {
                            for (text, delta) in [("-$1k", -1000.0), ("+$1k", 1000.0)] {
                                key(text, || {
                                    let (saved, ctx) = (saved.clone(), ctx.clone());
                                    styled_button(
                                        Modifier::new(),
                                        ButtonStyle::Outline,
                                        text,
                                        None,
                                        None,
                                        move || {
                                            let next = (saved.get() + delta).clamp(0.0, goal);
                                            saved.set(next);
                                            if next >= goal {
                                                ctx.say("Goal reached");
                                            }
                                        },
                                    );
                                });
                            }
                        },
                    );
                },
            );
            spacer(Modifier::new().height(10.0));
            meter(percent);
            spacer(Modifier::new().height(8.0));
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpaceBetween,
                Alignment::Center,
                || {
                    muted_sized(&format!("{:.0}% achieved", percent * 100.0), 12.0);
                    title(&money(saved.get()), 12.0);
                },
            );
        },
    );
    spacer(Modifier::new().height(10.0));
}

#[composable]
fn holding_row(name: &str, shares: &str, bars: Vec<f32>, dividend: f64) {
    let theme = theme();
    let open = remember(|| false);
    let reinvest = remember(|| false);
    let toggle = open.clone();
    column_with(
        Modifier::new()
            .fill_max_width()
            .background_rounded(theme.secondary, 10.0)
            .border(1.0, theme.outline, 10.0),
        || {
            row_arranged(
                Modifier::new()
                    .fill_max_width()
                    .semantics(
                        Semantics::new(Role::Button)
                            .label(name.to_string())
                            .merging_descendants(),
                    )
                    .clickable(move || toggle.update(|o| *o = !*o))
                    .padding_xy(16.0, 12.0),
                Arrangement::SpaceBetween,
                Alignment::Center,
                || {
                    column(|| {
                        title(name, 14.0);
                        muted_sized(shares, 12.0);
                    });
                    row_arranged(
                        Modifier::new(),
                        Arrangement::SpacedBy(10.0),
                        Alignment::Center,
                        || {
                            bar_chart(
                                Modifier::new().size(64.0, 28.0),
                                bars,
                                Vec::new(),
                                theme.on_surface_muted,
                                theme.outline,
                                None,
                                false,
                                chart_style(),
                            );
                            icon(
                                if open.get() {
                                    Icon::ChevronUp
                                } else {
                                    Icon::ChevronDown
                                },
                                14.0,
                                theme.on_surface_muted,
                            );
                        },
                    );
                },
            );
            if open.get() {
                column_with(
                    Modifier::new()
                        .fill_max_width()
                        .padding_each(16.0, 0.0, 16.0, 14.0),
                    || {
                        separator();
                        key_value("Q2 dividend", &money(dividend), false);
                        key_value("Next payout", "Jun 28", false);
                        row_arranged(
                            Modifier::new().fill_max_width().padding_xy(0.0, 4.0),
                            Arrangement::SpaceBetween,
                            Alignment::Center,
                            || {
                                muted_sized("Reinvest dividends", 13.0);
                                let flip = reinvest.clone();
                                switch(reinvest.get(), move |v| flip.set(v));
                            },
                        );
                    },
                );
            }
        },
    );
    spacer(Modifier::new().height(8.0));
}

#[composable]
fn first_column(ctx: &Ctx) {
    let theme = theme();
    let name = remember_text_field_state("");
    let message = remember_text_field_state("");
    let radios = remember(|| 0usize);
    let checked = remember(|| true);
    let toggles = remember(|| (false, true));
    let retirement = remember(|| 273_000.0_f64);
    let estate = remember(|| 27_200.0_f64);
    let filter = name.text();

    panel("", "", || {
        row_arranged(
            Modifier::new(),
            Arrangement::SpacedBy(8.0),
            Alignment::Center,
            || {
                for (label, style, trailing) in [
                    ("Button", ButtonStyle::Default, Some(Icon::ArrowRight)),
                    ("Secondary", ButtonStyle::Secondary, None),
                    ("Outline", ButtonStyle::Outline, None),
                ] {
                    key(label, || {
                        let ctx = ctx.clone();
                        styled_button(Modifier::new(), style, label, None, trailing, move || {
                            ctx.say(format!("{label} button clicked"))
                        });
                    });
                }
            },
        );
        spacer(Modifier::new().height(16.0));
        stack_aligned(
            Modifier::new().fill_max_width(),
            BoxAlignment::CenterEnd,
            || {
                text_field(Modifier::new().fill_max_width(), &name, "Name");
                stack_with(Modifier::new().padding_xy(12.0, 0.0), || {
                    icon(Icon::Search, 15.0, theme.on_surface_muted);
                });
            },
        );
        spacer(Modifier::new().height(10.0));
        text_area(
            Modifier::new().fill_max_width().height(80.0),
            &message,
            "Message",
        );
        spacer(Modifier::new().height(16.0));
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceBetween,
            Alignment::Center,
            || {
                row_arranged(
                    Modifier::new(),
                    Arrangement::SpacedBy(6.0),
                    Alignment::Center,
                    || {
                        pill("Badge", BadgeStyle::Default);
                        pill("Secondary", BadgeStyle::Secondary);
                    },
                );
                row_arranged(
                    Modifier::new(),
                    Arrangement::SpacedBy(10.0),
                    Alignment::Center,
                    || {
                        for index in 0..2 {
                            key(index, || {
                                let pick = radios.clone();
                                radio_button(radios.get() == index, move || pick.set(index));
                            });
                        }
                        let flip = checked.clone();
                        checkbox(checked.get(), move |value| flip.set(value));
                        let (first, second) = toggles.get();
                        let one = toggles.clone();
                        let two = toggles.clone();
                        switch(first, move |value| one.update(|t| t.0 = value));
                        switch(second, move |value| two.update(|t| t.1 = value));
                    },
                );
            },
        );
        spacer(Modifier::new().height(16.0));
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceBetween,
            Alignment::Center,
            || {
                let alert = ctx.clone();
                styled_button(
                    Modifier::new(),
                    ButtonStyle::Ghost,
                    "Alert Dialog",
                    None,
                    None,
                    move || alert.open(Overlay::Alert),
                );
                let group = [
                    ("Archive", Icon::Folder),
                    ("Mark as read", Icon::Check),
                    ("Snooze", Icon::Calendar),
                    ("Add to favorites", Icon::Plus),
                ]
                .map(|(label, glyph)| {
                    let ctx = ctx.clone();
                    MenuItem::new(label, move || ctx.say(format!("{label} done"))).icon(glyph)
                })
                .to_vec();
                menu(
                    |open| {
                        styled_button(
                            Modifier::new(),
                            ButtonStyle::Outline,
                            "Button Group",
                            None,
                            Some(Icon::ChevronDown),
                            move || open(),
                        )
                    },
                    group,
                );
            },
        );
    });

    for (heading_a, entries_a, heading_b, entries_b) in [
        (
            "Planning",
            &[
                (Icon::File, "Documents"),
                (Icon::Wallet, "Budget"),
                (Icon::Receipt, "Reports"),
                (Icon::Circle, "Goals"),
                (Icon::Calendar, "Calendar"),
            ][..],
            "Support",
            &[
                (Icon::Help, "Help Center"),
                (Icon::File, "Docs"),
                (Icon::Mail, "Contact Us"),
                (Icon::Activity, "Status"),
                (Icon::Users, "Community"),
            ][..],
        ),
        (
            "Overview",
            &[
                (Icon::BarChart, "Analytics"),
                (Icon::Receipt, "Transactions"),
                (Icon::TrendingUp, "Investments"),
                (Icon::Wallet, "Accounts"),
                (Icon::CreditCard, "Spending"),
            ][..],
            "Account",
            &[
                (Icon::User, "Profile"),
                (Icon::CreditCard, "Billing"),
                (Icon::Bell, "Notifications"),
                (Icon::Lock, "Security"),
                (Icon::Settings, "Appearance"),
            ][..],
        ),
    ] {
        key(heading_a, || {
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpacedBy(12.0),
                Alignment::Start,
                || {
                    nav_list(heading_a, entries_a, &filter, ctx);
                    nav_list(heading_b, entries_b, &filter, ctx);
                },
            );
            spacer(Modifier::new().height(12.0));
        });
    }
    spacer(Modifier::new().height(4.0));
    panel(
        "Savings Targets",
        "Active milestones for 2024 across your portfolio. Monitor how close you are to each savings goal.",
        || {
            target_card(ctx, "RETIREMENT", 420_000.0, retirement.clone());
            target_card(ctx, "REAL ESTATE", 85_000.0, estate.clone());
        },
    );
}

#[composable]
fn second_column(ctx: &Ctx) {
    let theme = theme();
    let months = ["Dec", "Jan", "Feb", "Mar", "Apr", "May"];
    let amounts = [5200.0, 7400.0, 6600.0, 8200.0, 5600.0, 7800.0];
    let (hover, hovered) = bar_hover(months.len());
    let pinned = remember(|| None::<usize>);
    let plan = remember(|| true);
    let shown = hovered.or(pinned.get());
    panel("Contribution History", "Last 6 months of activity", || {
        muted_sized(
            &match shown {
                Some(i) => format!("{}: {}", months[i], money(amounts[i])),
                None => "Hover or click a month".to_string(),
            },
            13.0,
        );
        spacer(Modifier::new().height(8.0));
        let pin = pinned.clone();
        bar_chart(
            Modifier::new()
                .fill_max_width()
                .height(170.0)
                .chain(hover)
                .clickable(move || {
                    // The bar the pointer was over when it was pressed stays picked.
                    pin.set(hovered)
                }),
            vec![52.0, 74.0, 66.0, 82.0, 56.0, 78.0],
            months.map(String::from).to_vec(),
            theme.primary,
            theme.accent,
            shown.map(|i| (i, theme.on_surface)),
            true,
            chart_style(),
        );
        spacer(Modifier::new().height(12.0));
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpacedBy(10.0),
            Alignment::Start,
            || {
                let (c, flip) = (ctx.clone(), plan.clone());
                column_with(
                    tinted(Modifier::new().weight(1.0), move || {
                        c.say("Next contribution: May 2024")
                    }),
                    || {
                        muted_sized("UPCOMING", 11.0);
                        title("May 2024", 15.0);
                        muted_sized("Scheduled", 12.0);
                    },
                );
                let c = ctx.clone();
                column_with(
                    tinted(Modifier::new().weight(1.0), move || {
                        flip.update(|p| *p = !*p);
                        c.say("Savings plan changed");
                    }),
                    || {
                        muted_sized("SAVINGS PLAN", 11.0);
                        title(
                            if plan.get() {
                                "Accelerated"
                            } else {
                                "Standard"
                            },
                            15.0,
                        );
                        muted_sized("Recurring", 12.0);
                    },
                );
            },
        );
        spacer(Modifier::new().height(12.0));
        let report = ctx.clone();
        styled_button(
            Modifier::new().fill_max_width(),
            ButtonStyle::Default,
            "View Full Report",
            None,
            None,
            move || report.open(Overlay::Report),
        );
    });
    panel("", "", || {
        muted("Claimable Balance");
        spacer(Modifier::new().height(6.0));
        row_arranged(
            Modifier::new(),
            Arrangement::SpacedBy(10.0),
            Alignment::Center,
            || {
                title("$1,211.29", 30.0);
                let pending = ctx.clone();
                stack_with(
                    Modifier::new()
                        .semantics(Semantics::new(Role::Button).label("Pending Setup".to_string()))
                        .clickable(move || pending.open(Overlay::Pending)),
                    || pill("Pending Setup", BadgeStyle::Secondary),
                );
            },
        );
        spacer(Modifier::new().height(14.0));
        separator();
        key_value("Net Royalties", "$1,248.75", false);
        key_value("Processing Fee", "-$37.46", false);
        separator();
        key_value("Total Ready to Claim", "$1,211.29 USD", true);
        spacer(Modifier::new().height(10.0));
        muted_sized(
            "Once your bank is connected, balances over $10.00 are automatically eligible for monthly distribution on the 15th of each month.",
            12.0,
        );
    });
    panel(
        "Q2 Dividend Income",
        "Quarterly dividend payouts across your portfolio holdings.",
        || {
            holding_row(
                "Vanguard",
                "450 Shares",
                vec![20.0, 26.0, 22.0, 34.0, 38.0],
                612.40,
            );
            holding_row(
                "S&P 500 VOO",
                "112 Shares",
                vec![24.0, 30.0, 28.0, 40.0, 36.0],
                204.15,
            );
            holding_row(
                "Apple AAPL",
                "86 Shares",
                vec![18.0, 22.0, 26.0, 24.0, 30.0],
                21.50,
            );
        },
    );
}

fn account_names(balances: [f64; 3]) -> Vec<String> {
    ACCOUNTS
        .iter()
        .zip(balances)
        .map(|((name, last), balance)| format!("{name} ({last}) \u{2014} {}", money(balance)))
        .collect()
}

#[composable]
fn third_column(ctx: &Ctx) {
    let theme = theme();
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
                    .background_rounded(theme.secondary, 10.0)
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
                icon(Icon::ChevronRight, 12.0, theme.on_surface_muted);
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
                            || icon(Icon::MoreHorizontal, 14.0, theme.on_surface_muted),
                        )
                    },
                    trail,
                );
                icon(Icon::ChevronRight, 12.0, theme.on_surface_muted);
                text("Payments".to_string());
            },
        );
        spacer(Modifier::new().height(14.0));
        for (index, (glyph, heading, detail)) in [
            (
                Icon::TrendingUp,
                "Change transfer limit",
                "Adjust how much you can send from your balance.",
            ),
            (
                Icon::Calendar,
                "Scheduled transfers",
                "Set up a transfer to send at a later date.",
            ),
            (
                Icon::CreditCard,
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
                        .background_rounded(theme.secondary, 10.0)
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
                                        icon(glyph, 16.0, theme.on_surface_muted);
                                        column_with(Modifier::new().width_in(120.0, 260.0), || {
                                            title(heading, 13.0);
                                            muted_sized(detail, 12.0);
                                        });
                                    },
                                );
                                icon(
                                    if open {
                                        Icon::ChevronDown
                                    } else {
                                        Icon::ChevronRight
                                    },
                                    14.0,
                                    theme.on_surface_muted,
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
                                Some(Icon::ArrowRight),
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

#[composable]
pub fn components_page(ctx: &Ctx) {
    row_arranged(
        Modifier::new().fill_max_width().padding(24.0),
        Arrangement::SpacedBy(16.0),
        Alignment::Start,
        || {
            column_with(Modifier::new().weight(1.0), || first_column(ctx));
            column_with(Modifier::new().weight(1.0), || second_column(ctx));
            column_with(Modifier::new().weight(1.0), || third_column(ctx));
        },
    );
}
