//! Cards and tables.

use rustle::prelude::*;

use crate::ctx::Ctx;
use crate::ui::panel;

#[composable]
pub(super) fn login_card(ctx: &Ctx) {
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
pub(super) fn tables() {
    let theme = color_scheme();
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
                                theme.surface_variant
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
