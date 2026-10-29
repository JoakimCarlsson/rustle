//! A catalogue in the manner of shadcn/ui's component docs: one small card per
//! component, each with its variants, each one working.

use rustle::prelude::*;

use crate::ctx::{Ctx, Overlay};
use crate::pages::page_frame;
use crate::ui::{icon_action, panel};

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

// --- Button ----------------------------------------------------------------

#[composable]
fn buttons(ctx: &Ctx) {
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
                                .background_rounded(theme().accent, 8.0),
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
fn badges() {
    let theme = theme();
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
                            .background_rounded(theme.success, 4.0),
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
                    icon(phosphor::CHECK, 12.0, theme.success);
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
fn avatars() {
    let theme = theme();
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

// --- Alert -----------------------------------------------------------------

#[composable]
fn callout(glyph: Icon, heading: &str, body: &str, destructive: bool) {
    let theme = theme();
    let color = if destructive {
        theme.danger
    } else {
        theme.on_surface
    };
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .border(
                1.0,
                if destructive {
                    theme.danger
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
fn alerts() {
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

// --- Forms -----------------------------------------------------------------

#[composable]
fn inputs(ctx: &Ctx) {
    let email = remember_text_field_state("");
    let note = remember_text_field_state("");
    let subscribed = remember(|| false);
    let ctx = ctx.clone();
    panel(
        "Input",
        "Fields with a label, a button, and a validation message.",
        || {
            let touched = remember(|| false);
            let invalid = touched.get() && !email.text().contains('@');
            let submit = {
                let (email, ctx, touched) = (email.clone(), ctx.clone(), touched.clone());
                move || {
                    touched.set(true);
                    if email.text().contains('@') {
                        ctx.say(format!("Subscribed {}", email.text()));
                        email.clear();
                        touched.set(false);
                    }
                }
            };
            email.on_submit(submit.clone());
            muted_sized("Email", 12.0);
            spacer(Modifier::new().height(6.0));
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpacedBy(8.0),
                Alignment::Center,
                || {
                    stack_with(Modifier::new().weight(1.0), || {
                        text_field(Modifier::new().fill_max_width(), &email, "m@example.com");
                    });
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Default,
                        "Subscribe",
                        None,
                        None,
                        submit,
                    );
                },
            );
            animated_visibility(invalid, || {
                spacer(Modifier::new().height(6.0));
                text_styled(
                    Modifier::new(),
                    "Enter a valid email address.".to_string(),
                    LOCAL_TEXT_STYLE
                        .current()
                        .color(theme().danger)
                        .font_size(12.0),
                );
            });
            spacer(Modifier::new().height(14.0));
            muted_sized("Message", 12.0);
            spacer(Modifier::new().height(6.0));
            text_area(
                Modifier::new().fill_max_width().height(76.0),
                &note,
                "Type your message here.",
            );
            spacer(Modifier::new().height(12.0));
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(10.0),
                Alignment::Center,
                || {
                    let on = subscribed.clone();
                    checkbox(subscribed.get(), move |value| on.set(value));
                    text("Accept terms and conditions".to_string());
                },
            );
        },
    );
}

#[composable]
fn choices() {
    let plan = remember(|| 0_usize);
    let notify = remember(|| true);
    let marketing = remember(|| false);
    let volume = remember(|| 0.4_f32);
    let range = remember(|| (0.2_f32, 0.7_f32));
    let size = remember(|| 1_usize);
    panel(
        "Radio, switch, slider",
        "Pick one, flip it, drag it.",
        || {
            for (index, (name, detail)) in [
                ("Starter", "For side projects"),
                ("Team", "For growing teams"),
                ("Enterprise", "Custom limits and support"),
            ]
            .into_iter()
            .enumerate()
            {
                key(index, || {
                    let pick = plan.clone();
                    let again = plan.clone();
                    row_arranged(
                        Modifier::new()
                            .fill_max_width()
                            .clickable_plain(move || again.set(index))
                            .padding_xy(0.0, 6.0),
                        Arrangement::SpacedBy(12.0),
                        Alignment::Center,
                        || {
                            radio_button(plan.get() == index, move || pick.set(index));
                            column(|| {
                                title(name, 13.0);
                                muted_sized(detail, 12.0);
                            });
                        },
                    );
                });
            }
            spacer(Modifier::new().height(6.0));
            divider();
            spacer(Modifier::new().height(10.0));
            for (label, state) in [
                ("Push notifications", &notify),
                ("Marketing emails", &marketing),
            ] {
                key(label, || {
                    row_arranged(
                        Modifier::new().fill_max_width().padding_xy(0.0, 4.0),
                        Arrangement::SpaceBetween,
                        Alignment::Center,
                        || {
                            text(label.to_string());
                            let toggle = state.clone();
                            switch(state.get(), move |value| toggle.set(value));
                        },
                    );
                });
            }
            spacer(Modifier::new().height(10.0));
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpaceBetween,
                Alignment::Center,
                || {
                    muted_sized("Volume", 12.0);
                    title(&format!("{:.0}", volume.get() * 100.0), 12.0);
                },
            );
            let set = volume.clone();
            slider(260.0, volume.get(), move |value| set.set(value));
            spacer(Modifier::new().height(10.0));
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpaceBetween,
                Alignment::Center,
                || {
                    muted_sized("Price range", 12.0);
                    title(
                        &format!(
                            "${:.0} to ${:.0}",
                            range.get().0 * 500.0,
                            range.get().1 * 500.0
                        ),
                        12.0,
                    );
                },
            );
            let set = range.clone();
            range_slider(260.0, range.get().0, range.get().1, move |a, b| {
                set.set((a, b))
            });
            spacer(Modifier::new().height(14.0));
            muted_sized("Toggle group", 12.0);
            spacer(Modifier::new().height(6.0));
            let pick = size.clone();
            segmented_control(size.get(), &["S", "M", "L", "XL"], move |index| {
                pick.set(index)
            });
        },
    );
}

#[composable]
fn selects(ctx: &Ctx) {
    let fruit = remember(|| 0_usize);
    let ctx = ctx.clone();
    panel(
        "Select, menu and tooltip",
        "Choose from a list, or act on one.",
        || {
            let options: Vec<String> = ["Apple", "Banana", "Blueberry", "Grapes", "Pineapple"]
                .iter()
                .map(|s| s.to_string())
                .collect();
            let pick = fruit.clone();
            select(
                Modifier::new().width(220.0),
                options,
                fruit.get(),
                move |index| pick.set(index),
            );
            spacer(Modifier::new().height(12.0));
            wrap(|| {
                let items = vec![
                    MenuItem::new("Profile", {
                        let ctx = ctx.clone();
                        move || ctx.say("Profile")
                    })
                    .icon(phosphor::USER),
                    MenuItem::new("Billing", {
                        let ctx = ctx.clone();
                        move || ctx.say("Billing")
                    })
                    .icon(phosphor::CREDIT_CARD),
                    MenuItem::new("Delete account", {
                        let ctx = ctx.clone();
                        move || ctx.say("Not today")
                    })
                    .icon(phosphor::X)
                    .destructive(),
                ];
                menu(
                    |open| {
                        styled_button(
                            Modifier::new(),
                            ButtonStyle::Outline,
                            "Open menu",
                            None,
                            Some(phosphor::CARET_DOWN),
                            move || open(),
                        );
                    },
                    items,
                );
                tooltip("Add to library", || {
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Outline,
                        "Hover",
                        Some(phosphor::PLUS),
                        None,
                        || {},
                    );
                });
            });
        },
    );
}

#[composable]
fn otp() {
    let theme = theme();
    let code = remember_text_field_state("");
    panel("Input OTP", "A six digit code, one box per digit.", || {
        let digits: Vec<char> = code
            .text()
            .chars()
            .filter(char::is_ascii_digit)
            .take(6)
            .collect();
        stack_with(Modifier::new(), || {
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(8.0),
                Alignment::Center,
                || {
                    for index in 0..6 {
                        key(index, || {
                            let current = index == digits.len();
                            let outline = if current {
                                theme.primary
                            } else {
                                theme.outline
                            };
                            stack_aligned(
                                Modifier::new()
                                    .size(40.0, 46.0)
                                    .background_rounded(theme.secondary, 8.0)
                                    .border(if current { 1.5 } else { 1.0 }, outline, 8.0),
                                BoxAlignment::Center,
                                || {
                                    if let Some(digit) = digits.get(index) {
                                        title(&digit.to_string(), 18.0);
                                    }
                                },
                            );
                        });
                    }
                },
            );
            // The real field sits invisibly on top and takes the typing.
            stack_with(Modifier::new().size(328.0, 46.0).alpha(0.0), || {
                text_field(Modifier::new().fill_max_size(), &code, "");
            });
        });
        spacer(Modifier::new().height(8.0));
        muted_sized(
            if digits.len() == 6 {
                "Code complete."
            } else {
                "Click the boxes and type."
            },
            12.0,
        );
    });
}

// --- Navigation --------------------------------------------------------------

#[composable]
fn navigation() {
    let theme = theme();
    let page = remember(|| 2_usize);
    let tab = remember(|| 0_usize);
    panel(
        "Breadcrumb, tabs, pagination",
        "Where you are, and how to move.",
        || {
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(6.0),
                Alignment::Center,
                || {
                    muted_sized("Home", 13.0);
                    icon(phosphor::CARET_RIGHT, 13.0, theme.on_surface_muted);
                    muted_sized("Components", 13.0);
                    icon(phosphor::CARET_RIGHT, 13.0, theme.on_surface_muted);
                    title("Breadcrumb", 13.0);
                },
            );
            spacer(Modifier::new().height(16.0));
            let pick = tab.clone();
            tabs_list(
                tab.get(),
                &[("Account", None), ("Password", None), ("Billing", Some(2))],
                move |index| pick.set(index),
            );
            spacer(Modifier::new().height(8.0));
            stack_with(Modifier::new().height(24.0), || {
                muted_sized(
                    [
                        "Make changes to your account.",
                        "Change your password here.",
                        "Two invoices are due.",
                    ][tab.get()],
                    13.0,
                );
            });
            spacer(Modifier::new().height(10.0));
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(4.0),
                Alignment::Center,
                || {
                    let back = page.clone();
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Ghost,
                        "Previous",
                        Some(phosphor::CARET_LEFT),
                        None,
                        move || back.update(|p| *p = p.saturating_sub(1)),
                    );
                    for number in 0..5 {
                        key(number, || {
                            let pick = page.clone();
                            let active = page.get() == number;
                            stack_aligned(
                                Modifier::new()
                                    .size(34.0, 34.0)
                                    .semantics(
                                        Semantics::new(Role::Button)
                                            .label(format!("Page {}", number + 1)),
                                    )
                                    .clickable(move || pick.set(number))
                                    .border(
                                        1.0,
                                        if active {
                                            theme.outline
                                        } else {
                                            Color::TRANSPARENT
                                        },
                                        8.0,
                                    ),
                                BoxAlignment::Center,
                                || text((number + 1).to_string()),
                            );
                        });
                    }
                    let next = page.clone();
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Ghost,
                        "Next",
                        None,
                        Some(phosphor::CARET_RIGHT),
                        move || next.update(|p| *p = (*p + 1).min(4)),
                    );
                },
            );
        },
    );
}

// --- Cards and tables --------------------------------------------------------

#[composable]
fn login_card(ctx: &Ctx) {
    let email = remember_text_field_state("");
    let password = remember_text_field_state("");
    let ctx = ctx.clone();
    panel("Card", "A login form, the way the docs show one.", || {
        column_with(Modifier::new().fill_max_width(), || {
            title("Login to your account", 15.0);
            muted_sized("Enter your email below to log in.", 13.0);
            spacer(Modifier::new().height(14.0));
            muted_sized("Email", 12.0);
            spacer(Modifier::new().height(6.0));
            text_field(Modifier::new().fill_max_width(), &email, "m@example.com");
            spacer(Modifier::new().height(12.0));
            muted_sized("Password", 12.0);
            spacer(Modifier::new().height(6.0));
            text_field(Modifier::new().fill_max_width(), &password, "Your password");
            spacer(Modifier::new().height(16.0));
            let submit = ctx.clone();
            styled_button(
                Modifier::new().fill_max_width(),
                ButtonStyle::Default,
                "Login",
                None,
                None,
                move || submit.say("Logged in (not really)"),
            );
            spacer(Modifier::new().height(8.0));
            let google = ctx.clone();
            styled_button(
                Modifier::new().fill_max_width(),
                ButtonStyle::Outline,
                "Login with Google",
                None,
                None,
                move || google.say("Google says no"),
            );
        });
    });
}

#[composable]
fn tables() {
    let theme = theme();
    let rows = [
        ("INV001", "Paid", "Credit Card", "$250.00"),
        ("INV002", "Pending", "PayPal", "$150.00"),
        ("INV003", "Unpaid", "Bank Transfer", "$350.00"),
        ("INV004", "Paid", "Credit Card", "$450.00"),
    ];
    let selected = remember(|| None::<usize>);
    panel("Table", "Click a row to select it.", || {
        column_with(
            Modifier::new()
                .fill_max_width()
                .border(1.0, theme.outline, 10.0)
                .clip(),
            || {
                table_row(|| {
                    for (weight, label) in [
                        (1.0, "Invoice"),
                        (1.0, "Status"),
                        (1.4, "Method"),
                        (0.9, "Amount"),
                    ] {
                        table_cell(weight, || muted_sized(label, 12.0));
                    }
                });
                for (index, (id, status, method, amount)) in rows.into_iter().enumerate() {
                    key(index, || {
                        let pick = selected.clone();
                        let fill = animate_color_as_state(
                            if selected.get() == Some(index) {
                                theme.accent
                            } else {
                                Color::TRANSPARENT
                            },
                            tween(150),
                        )
                        .get();
                        stack_with(
                            Modifier::new()
                                .fill_max_width()
                                .clickable_plain(move || pick.set(Some(index)))
                                .background(fill),
                            || {
                                table_row(|| {
                                    table_cell(1.0, || title(id, 13.0));
                                    table_cell(1.0, || {
                                        pill(
                                            status,
                                            if status == "Paid" {
                                                BadgeStyle::Default
                                            } else {
                                                BadgeStyle::Outline
                                            },
                                        )
                                    });
                                    table_cell(1.4, || text(method.to_string()));
                                    table_cell(0.9, || text(amount.to_string()));
                                });
                            },
                        );
                    });
                }
            },
        );
    });
}

// --- Overlays ----------------------------------------------------------------

#[composable]
fn collapsible() {
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
fn repo_row(name: &str) {
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
fn hover_card() {
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
fn overlays(ctx: &Ctx) {
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
fn calendars() {
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
                    });
                    column_with(Modifier::new().weight(1.0), || {
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
