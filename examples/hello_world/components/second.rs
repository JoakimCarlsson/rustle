//! The second column: holdings and the rest.

use rustle::prelude::*;

use crate::ctx::{Ctx, Overlay};
use crate::ui::{chart_style, key_value, money, panel, tinted};

#[composable]
pub(super) fn holding_row(name: &str, shares: &str, bars: Vec<f32>, dividend: f64) {
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
                                    phosphor::CARET_UP
                                } else {
                                    phosphor::CARET_DOWN
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
pub(super) fn second_column(ctx: &Ctx) {
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
