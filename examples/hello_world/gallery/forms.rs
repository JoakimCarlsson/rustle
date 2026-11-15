//! Form controls: inputs, choices, selects and OTP.

use rustle::prelude::*;

use super::wrap;
use crate::ctx::Ctx;
use crate::ui::panel;

#[composable]
pub(super) fn inputs(ctx: &Ctx) {
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
                        .color(color_scheme().error)
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
pub(super) fn choices() {
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
pub(super) fn selects(ctx: &Ctx) {
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
pub(super) fn otp() {
    let theme = color_scheme();
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
                                    .background_rounded(theme.secondary_container, 8.0)
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
