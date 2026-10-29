//! Dialogs for creating and inviting.

use rustle::prelude::*;

use super::{dialog_buttons, dialog_heading};
use crate::ctx::{Ctx, KINDS};

#[composable]
pub(super) fn new_document(ctx: &Ctx) {
    let name = remember_text_field_state("");
    let kind = remember(|| 0usize);
    let focus = remember_focus();
    let asked = remember(|| false);
    if !asked.get() {
        asked.set(true);
        focus.request_focus();
    }
    let create = {
        let (ctx, name, kind) = (ctx.clone(), name.clone(), kind.clone());
        move || {
            let header = name.text();
            if header.trim().is_empty() {
                ctx.say("Give the section a name first");
                return;
            }
            ctx.add_doc(header.trim(), KINDS[kind.get()]);
            ctx.close();
            ctx.go(0);
            ctx.say(format!("Created \"{}\"", header.trim()));
        }
    };
    name.on_submit(create.clone());
    dialog(
        {
            let ctx = ctx.clone();
            move || ctx.close()
        },
        || {
            column_with(Modifier::new().width(380.0), || {
                dialog_heading("Add section", "Add a section to the document outline.");
                muted_sized("Header", 12.0);
                spacer(Modifier::new().height(6.0));
                text_field_focused(
                    Modifier::new().fill_max_width(),
                    &name,
                    "Section name",
                    &focus,
                );
                spacer(Modifier::new().height(12.0));
                muted_sized("Section type", 12.0);
                spacer(Modifier::new().height(6.0));
                let pick = kind.clone();
                select(
                    Modifier::new().fill_max_width(),
                    KINDS.iter().map(|k| k.to_string()).collect(),
                    kind.get(),
                    move |index| pick.set(index),
                );
                dialog_buttons("Create", create, {
                    let ctx = ctx.clone();
                    move || ctx.close()
                });
            });
        },
    );
}

#[composable]
pub(super) fn invite(ctx: &Ctx) {
    let email = remember_text_field_state("");
    let role = remember(|| 1usize);
    let send = {
        let (ctx, email, role) = (ctx.clone(), email.clone(), role.clone());
        move || {
            let address = email.text();
            if !address.contains('@') {
                ctx.say("Enter a valid email address");
                return;
            }
            let name = address.split('@').next().unwrap_or("").to_string();
            ctx.members.push(crate::ctx::Member {
                name: name.clone(),
                email: address.clone(),
                role: role.get(),
            });
            ctx.close();
            ctx.say(format!("Invited {address}"));
        }
    };
    email.on_submit(send.clone());
    dialog(
        {
            let ctx = ctx.clone();
            move || ctx.close()
        },
        || {
            column_with(Modifier::new().width(380.0), || {
                dialog_heading(
                    "Invite member",
                    "They will get an email with a link to join.",
                );
                text_field(Modifier::new().fill_max_width(), &email, "name@example.com");
                spacer(Modifier::new().height(12.0));
                let pick = role.clone();
                select(
                    Modifier::new().fill_max_width(),
                    crate::ctx::ROLES.iter().map(|r| r.to_string()).collect(),
                    role.get(),
                    move |index| pick.set(index),
                );
                dialog_buttons("Send invite", send, {
                    let ctx = ctx.clone();
                    move || ctx.close()
                });
            });
        },
    );
}

#[composable]
pub(super) fn pending_setup(ctx: &Ctx) {
    dialog_heading(
        "Connect your bank",
        "Balances over $10.00 are paid out on the 15th once a bank is connected.",
    );
    let theme = theme();
    for (label, detail) in [
        ("Checking account", "Ending in 8402"),
        ("Savings account", "Ending in 1192"),
    ] {
        key(label, || {
            let ctx = ctx.clone();
            row_arranged(
                Modifier::new()
                    .width(340.0)
                    .clickable(move || {
                        ctx.close();
                        ctx.say(format!("{label} connected"));
                    })
                    .background_rounded(theme.secondary, 10.0)
                    .padding(14.0),
                Arrangement::SpaceBetween,
                Alignment::Center,
                || {
                    column(|| {
                        title(label, 14.0);
                        muted_sized(detail, 12.0);
                    });
                    icon(phosphor::CARET_RIGHT, 14.0, theme.on_surface_muted);
                },
            );
            spacer(Modifier::new().height(8.0));
        });
    }
    let ctx = ctx.clone();
    styled_button(
        Modifier::new(),
        ButtonStyle::Ghost,
        "Maybe later",
        None,
        None,
        move || ctx.close(),
    );
}
