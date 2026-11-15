//! The first column: navigation lists and savings targets.

use rustle::prelude::*;

use crate::ctx::{Ctx, Overlay};
use crate::ui::{meter, money, panel};

type Entry = (Icon, &'static str);

#[composable]
pub(super) fn nav_list(heading: &str, entries: &[Entry], filter: &str, toast: &Ctx) {
    let theme = color_scheme();
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
                        theme.on_surface_variant
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
                                    theme.surface_variant
                                } else {
                                    Color::TRANSPARENT
                                },
                                6.0,
                            )
                            .padding_xy(8.0, 7.0),
                        Arrangement::SpacedBy(10.0),
                        Alignment::Center,
                        || {
                            icon(glyph.clone(), 15.0, color);
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
pub(super) fn target_card(ctx: &Ctx, label: &str, goal: f64, saved: State<f64>) {
    let theme = color_scheme();
    let percent = (saved.get() / goal).clamp(0.0, 1.0) as f32;
    column_with(
        Modifier::new()
            .fill_max_width()
            .background_rounded(theme.secondary_container, 10.0)
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
pub(super) fn first_column(ctx: &Ctx) {
    let theme = color_scheme();
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
                    ("Button", ButtonStyle::Default, Some(phosphor::ARROW_RIGHT)),
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
                    icon(phosphor::MAGNIFYING_GLASS, 15.0, theme.on_surface_variant);
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
                    ("Archive", phosphor::FOLDER),
                    ("Mark as read", phosphor::CHECK),
                    ("Snooze", phosphor::CALENDAR_BLANK),
                    ("Add to favorites", phosphor::PLUS),
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
                            Some(phosphor::CARET_DOWN),
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
                (phosphor::FILE, "Documents"),
                (phosphor::WALLET, "Budget"),
                (phosphor::RECEIPT, "Reports"),
                (phosphor::CIRCLE, "Goals"),
                (phosphor::CALENDAR_BLANK, "Calendar"),
            ][..],
            "Support",
            &[
                (phosphor::QUESTION, "Help Center"),
                (phosphor::FILE, "Docs"),
                (phosphor::ENVELOPE_SIMPLE, "Contact Us"),
                (phosphor::PULSE, "Status"),
                (phosphor::USERS, "Community"),
            ][..],
        ),
        (
            "Overview",
            &[
                (phosphor::CHART_BAR, "Analytics"),
                (phosphor::RECEIPT, "Transactions"),
                (phosphor::TREND_UP, "Investments"),
                (phosphor::WALLET, "Accounts"),
                (phosphor::CREDIT_CARD, "Spending"),
            ][..],
            "Account",
            &[
                (phosphor::USER, "Profile"),
                (phosphor::CREDIT_CARD, "Billing"),
                (phosphor::BELL, "Notifications"),
                (phosphor::LOCK, "Security"),
                (phosphor::GEAR, "Appearance"),
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
