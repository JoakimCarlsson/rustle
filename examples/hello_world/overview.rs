//! A grid of small cards: charts that read out the point under the pointer,
//! team members, a chat, a calendar, a goal and some settings.

use rustle::prelude::*;

use crate::ctx::{Ctx, Overlay, ROLES};
use crate::ui::{chart_style, icon_box, panel};

const MONTHS: [&str; 12] = [
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

#[composable]
fn setting_row(heading: &str, detail: &str, on: State<bool>) {
    row_arranged(
        Modifier::new().fill_max_width().padding_xy(0.0, 8.0),
        Arrangement::SpaceBetween,
        Alignment::Center,
        || {
            column_with(Modifier::new().width_in(120.0, 300.0), || {
                title(heading, 14.0);
                muted_sized(detail, 13.0);
            });
            let toggle = on.clone();
            switch(on.get(), move |value| toggle.set(value));
        },
    );
}

#[composable]
fn revenue_card() {
    let theme = theme();
    let values: Vec<f32> = vec![42.0, 38.0, 44.0, 40.0, 41.0, 39.0, 45.0, 52.0, 55.0];
    let (hover, hovered) = chart_hover(values.len(), 8.0, 8.0);
    let shown = hovered.map_or(15_231.89, |i| f64::from(values[i]) * 290.0);
    panel("Total Revenue", "", || {
        column(|| {
            title(&crate::ui::money(shown), 26.0);
            muted_sized(
                &match hovered {
                    Some(i) => format!("Week {} of the last 9", i + 1),
                    None => "+20.1% from last month".to_string(),
                },
                13.0,
            );
            spacer(Modifier::new().height(12.0));
            line_chart(
                Modifier::new().fill_max_width().height(90.0).chain(hover),
                vec![Series::new(values, theme.primary).thickness(2.0)],
                vec![String::new(); 9],
                100,
                true,
                ChartStyle {
                    zero_baseline: false,
                    ..chart_style()
                },
            );
        });
    });
}

#[composable]
fn subscriptions_card() {
    let theme = theme();
    let values = vec![
        32.0, 40.0, 28.0, 46.0, 52.0, 44.0, 60.0, 36.0, 48.0, 30.0, 42.0, 54.0,
    ];
    let (hover, hovered) = bar_hover(values.len());
    let total: f32 = hovered.map_or(2350.0, |i| values[i] * 45.0);
    panel("Subscriptions", "", || {
        column(|| {
            title(&format!("+{}", total.round() as i64), 26.0);
            muted_sized(
                &match hovered {
                    Some(i) => format!("{} of the last 12", ordinal(i + 1)),
                    None => "+180.1% from last month".to_string(),
                },
                13.0,
            );
            spacer(Modifier::new().height(12.0));
            bar_chart(
                Modifier::new().fill_max_width().height(110.0).chain(hover),
                values,
                Vec::new(),
                theme.on_surface_muted,
                theme.on_surface_muted,
                hovered.map(|i| (i, theme.primary)),
                false,
                chart_style(),
            );
        });
    });
}

fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

#[composable]
fn team_panel(ctx: &Ctx) {
    panel(
        "Team Members",
        "Invite your team members to collaborate.",
        || {
            for (index, member) in ctx.members.to_vec().into_iter().enumerate() {
                key(member.email.clone(), || {
                    member_row(ctx, index, &member);
                });
            }
            spacer(Modifier::new().height(8.0));
            let ctx = ctx.clone();
            styled_button(
                Modifier::new().fill_max_width(),
                ButtonStyle::Outline,
                "Invite member",
                Some(phosphor::PLUS),
                None,
                move || ctx.open(Overlay::Invite),
            );
        },
    );
}

#[composable]
pub fn member_row(ctx: &Ctx, index: usize, member: &crate::ctx::Member) {
    let initials: String = member
        .name
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    row_arranged(
        Modifier::new().fill_max_width().padding_xy(0.0, 6.0),
        Arrangement::SpaceBetween,
        Alignment::Center,
        || {
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(12.0),
                Alignment::Center,
                || {
                    avatar(&initials, 36.0);
                    column(|| {
                        title(&member.name, 14.0);
                        muted_sized(&member.email, 13.0);
                    });
                },
            );
            let ctx = ctx.clone();
            select(
                Modifier::new().width(104.0),
                ROLES.iter().map(|r| r.to_string()).collect(),
                member.role,
                move |role| {
                    ctx.members.update(|list| {
                        if let Some(member) = list.get_mut(index) {
                            member.role = role;
                        }
                    });
                    ctx.say(format!("Role changed to {}", ROLES[role]));
                },
            );
        },
    );
}

#[composable]
fn chat_card(ctx: &Ctx) {
    let theme = theme();
    let epoch = remember(|| 0usize);
    card(Modifier::new().fill_max_width(), || {
        column_with(Modifier::new().fill_max_width().padding(20.0), || {
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
                            avatar("SD", 36.0);
                            column(|| {
                                title("Sofia Davis", 14.0);
                                muted_sized("m@example.com", 13.0);
                            });
                        },
                    );
                    let (fresh, ctx) = (epoch.clone(), ctx.clone());
                    icon_box(phosphor::PLUS, 30.0, "New conversation", move || {
                        fresh.update(|e| *e += 1);
                        ctx.say("Started a new conversation");
                    });
                },
            );
            spacer(Modifier::new().height(16.0));
            key(epoch.get(), || {
                crate::ui::chat(
                    &[
                        "Have you tried resetting your password from the sign-in page?",
                        "I have sent you a link, it is valid for the next hour.",
                        "Anything else I can help with today?",
                    ],
                    if epoch.get() == 0 {
                        &[
                            ("Hi, how can I help you today?", false),
                            ("Hey, I'm having trouble with my account.", true),
                            ("What seems to be the problem?", false),
                            ("I can't log in.", true),
                        ]
                    } else {
                        &[("Hi, how can I help you today?", false)]
                    },
                );
            });
        });
    });
    spacer(Modifier::new().height(16.0));
    let _ = theme;
}

fn month_start(year: i32, month: u32) -> (usize, usize) {
    (
        first_weekday(year, month) as usize,
        days_in_month(year, month) as usize,
    )
}

#[composable]
fn calendar_card(ctx: &Ctx) {
    let theme = theme();
    let month = remember(|| (2023i32, 6u32));
    let range = remember(|| (5usize, 13usize));
    let (year, number) = month.get();
    let (first, days) = month_start(year, number);
    panel("", "", || {
        spacer(Modifier::new().height(8.0));
        stack_aligned(
            Modifier::new().fill_max_width(),
            BoxAlignment::TopCenter,
            || {
                let (from, to) = range.get();
                let pick = range.clone();
                let step = month.clone();
                calendar(
                    &format!("{} {}", MONTHS[number as usize - 1], year),
                    first,
                    days,
                    Some((from.min(days), to.min(days))),
                    move |direction| {
                        step.update(|(y, m)| {
                            let index = (*y * 12 + *m as i32 - 1) + direction;
                            *y = index.div_euclid(12);
                            *m = index.rem_euclid(12) as u32 + 1;
                        })
                    },
                    move |day| {
                        let (from, to) = pick.get();
                        // A click before the range moves its start, after it moves its end.
                        if day < from || (day - from) < (to.max(day) - day) {
                            pick.set((day, to.max(day)));
                        } else {
                            pick.set((from, day));
                        }
                    },
                );
            },
        );
        spacer(Modifier::new().height(8.0));
        let (from, to) = range.get();
        muted_sized(
            &format!(
                "{} {} to {} {} ({} days)",
                &MONTHS[number as usize - 1][..3],
                from,
                &MONTHS[number as usize - 1][..3],
                to.min(days),
                to.min(days).saturating_sub(from) + 1
            ),
            12.0,
        );
        let _ = (theme, ctx);
    });
}

#[composable]
fn goal_card(ctx: &Ctx) {
    let theme = theme();
    let goal = remember(|| 350i32);
    let values = vec![
        40.0, 55.0, 35.0, 60.0, 45.0, 52.0, 38.0, 66.0, 58.0, 42.0, 70.0, 48.0, 62.0, 50.0,
    ];
    let (hover, hovered) = bar_hover(values.len());
    panel("Move Goal", "Set your daily activity goal.", || {
        column_with(Modifier::new().fill_max_width(), || {
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpaceBetween,
                Alignment::Center,
                || {
                    let down = goal.clone();
                    icon_box(phosphor::CARET_LEFT, 32.0, "Decrease goal", move || {
                        down.update(|g| *g = (*g - 10).max(0))
                    });
                    column_arranged(
                        Modifier::new(),
                        Arrangement::Start,
                        Alignment::Center,
                        || {
                            title(&goal.get().to_string(), 36.0);
                            muted_sized("CALORIES/DAY", 11.0);
                        },
                    );
                    let up = goal.clone();
                    icon_box(phosphor::CARET_RIGHT, 32.0, "Increase goal", move || {
                        up.update(|g| *g += 10)
                    });
                },
            );
            spacer(Modifier::new().height(12.0));
            bar_chart(
                Modifier::new().fill_max_width().height(60.0).chain(hover),
                values.clone(),
                Vec::new(),
                theme.on_surface_muted,
                theme.on_surface_muted,
                hovered.map(|i| (i, theme.primary)),
                false,
                chart_style(),
            );
            if let Some(i) = hovered {
                muted_sized(
                    &format!("Day {}: {} kcal", i + 1, (values[i] * 6.0) as i32),
                    12.0,
                );
            } else {
                muted_sized("Hover a bar to see a day", 12.0);
            }
            spacer(Modifier::new().height(12.0));
            let (ctx, goal) = (ctx.clone(), goal.clone());
            styled_button(
                Modifier::new().fill_max_width(),
                ButtonStyle::Default,
                "Set Goal",
                None,
                None,
                move || ctx.say(format!("Daily goal set to {} calories", goal.get())),
            );
        });
    });
}

#[composable]
fn exercise_card() {
    let theme = theme();
    let mine = vec![20.0, 25.0, 30.0, 90.0, 40.0, 25.0, 20.0];
    let (hover, hovered) = chart_hover(mine.len(), 8.0, 8.0);
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    panel(
        "Exercise Minutes",
        &match hovered {
            Some(i) => format!("{}: {} minutes", days[i], mine[i] as i32),
            None => "Your exercise minutes are ahead of where you normally are.".to_string(),
        },
        || {
            line_chart(
                Modifier::new().fill_max_width().height(150.0).chain(hover),
                vec![
                    Series::new(mine, theme.primary).thickness(2.0),
                    Series::new(
                        vec![32.0, 30.0, 34.0, 28.0, 36.0, 32.0, 30.0],
                        theme.on_surface_muted,
                    ),
                ],
                vec![String::new(); 7],
                100,
                true,
                chart_style(),
            );
        },
    );
}

#[composable]
pub fn overview_page(ctx: &Ctx) {
    let necessary = remember(|| true);
    let functional = remember(|| false);
    row_arranged(
        Modifier::new().fill_max_width().padding(24.0),
        Arrangement::SpacedBy(16.0),
        Alignment::Start,
        || {
            column_with(Modifier::new().weight(1.0), || {
                revenue_card();
                team_panel(ctx);
                panel(
                    "Cookie Settings",
                    "Manage your cookie settings here.",
                    || {
                        setting_row(
                            "Strictly Necessary",
                            "These cookies are essential in order to use the website.",
                            necessary.clone(),
                        );
                        setting_row(
                            "Functional Cookies",
                            "These cookies allow the website to provide personalization.",
                            functional.clone(),
                        );
                        spacer(Modifier::new().height(8.0));
                        let (ctx, necessary, functional) =
                            (ctx.clone(), necessary.clone(), functional.clone());
                        styled_button(
                            Modifier::new().fill_max_width(),
                            ButtonStyle::Outline,
                            "Save preferences",
                            None,
                            None,
                            move || {
                                ctx.say(format!(
                                    "Saved: necessary {}, functional {}",
                                    if necessary.get() { "on" } else { "off" },
                                    if functional.get() { "on" } else { "off" }
                                ))
                            },
                        );
                    },
                );
            });
            column_with(Modifier::new().weight(1.0), || {
                subscriptions_card();
                chat_card(ctx);
            });
            column_with(Modifier::new().weight(1.0), || {
                calendar_card(ctx);
                goal_card(ctx);
                exercise_card();
            });
        },
    );
}
