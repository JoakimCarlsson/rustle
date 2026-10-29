//! Everything that opens over a page: dialogs, sheets and the toast.

mod dialogs;
mod palette;
mod sheets;

use std::rc::Rc;

use rustle::prelude::*;

use crate::components::ACCOUNTS;
use crate::ctx::{Ctx, Overlay};
use crate::ui::money;
use dialogs::{invite, new_document, pending_setup};
use palette::search_palette;
use sheets::{detail_sheet, report_sheet};

#[composable]
pub fn overlays(ctx: &Ctx) {
    let close = {
        let ctx = ctx.clone();
        move || ctx.close()
    };
    match ctx.overlay.get() {
        Overlay::None => {}
        Overlay::Search => search_palette(ctx),
        Overlay::NewDocument => new_document(ctx),
        Overlay::Alert => confirm(
            "Are you absolutely sure?",
            "This action cannot be undone. This will permanently delete your account and remove your data from our servers.",
            "Continue",
            {
                let ctx = ctx.clone();
                move || {
                    ctx.close();
                    ctx.say("Account deletion cancelled in this demo");
                }
            },
            close,
        ),
        Overlay::Logout => confirm(
            "Log out?",
            "You will need to sign in again to see your dashboard.",
            "Log out",
            {
                let ctx = ctx.clone();
                move || {
                    ctx.close();
                    ctx.signed_in.set(false);
                }
            },
            close,
        ),
        Overlay::Transfer => {
            let (amount, from, to) = ctx.transfer.get();
            confirm(
                "Confirm transfer",
                &format!(
                    "Move {} from {} to {}?",
                    money(amount),
                    ACCOUNTS[from].0,
                    ACCOUNTS[to].0
                ),
                "Confirm Transfer",
                {
                    let ctx = ctx.clone();
                    move || {
                        let mut balances = ctx.balances.get();
                        balances[from] -= amount;
                        balances[to] += amount;
                        ctx.balances.set(balances);
                        ctx.close();
                        ctx.say(format!("Transferred {}", money(amount)));
                    }
                },
                close,
            );
        }
        Overlay::Pending => dialog(close, || pending_setup(ctx)),
        Overlay::Invite => invite(ctx),
        Overlay::Report | Overlay::Detail(_) => {}
    }

    // Sheets slide in and out, so they stay composed while they close.
    let report = ctx.overlay.get() == Overlay::Report;
    bottom_sheet(
        report,
        {
            let ctx = ctx.clone();
            move || ctx.close()
        },
        || report_sheet(ctx),
    );
    let detail = match ctx.overlay.get() {
        Overlay::Detail(id) => Some(id),
        _ => None,
    };
    let remembered = remember(|| 0usize);
    if let Some(id) = detail {
        remembered.set(id);
    }
    bottom_sheet(
        detail.is_some(),
        {
            let ctx = ctx.clone();
            move || ctx.close()
        },
        || detail_sheet(ctx, remembered.get()),
    );
    toast(ctx);
}

#[composable]
fn toast(ctx: &Ctx) {
    let theme = theme();
    let Some((_, message)) = ctx.toast.get() else {
        return;
    };
    stack_aligned(
        Modifier::new().fill_max_size().padding(24.0).z_index(2),
        BoxAlignment::BottomCenter,
        || {
            row_arranged(
                Modifier::new()
                    .shadow(8.0, 10.0)
                    .background_rounded(theme.surface, 10.0)
                    .border(1.0, theme.outline, 10.0)
                    .padding_xy(16.0, 12.0),
                Arrangement::SpacedBy(10.0),
                Alignment::Center,
                || {
                    icon(phosphor::CHECK, 15.0, theme.success);
                    text(message.clone());
                },
            );
        },
    );
}

/// Heading and description for a dialog.
#[composable]
fn dialog_heading(heading: &str, description: &str) {
    title(heading, 17.0);
    spacer(Modifier::new().height(4.0));
    muted_sized(description, 13.0);
    spacer(Modifier::new().height(16.0));
}

#[composable]
fn dialog_buttons(label: &str, on_confirm: impl Fn() + 'static, on_cancel: impl Fn() + 'static) {
    spacer(Modifier::new().height(18.0));
    row_arranged(
        Modifier::new().fill_max_width(),
        Arrangement::End,
        Alignment::Center,
        || {
            styled_button(
                Modifier::new(),
                ButtonStyle::Ghost,
                "Cancel",
                None,
                None,
                on_cancel,
            );
            spacer(Modifier::new().width(8.0));
            styled_button(
                Modifier::new(),
                ButtonStyle::Default,
                label,
                None,
                None,
                on_confirm,
            );
        },
    );
}

/// A dialog that asks whether to go ahead.
#[composable]
fn confirm(
    heading: &str,
    message: &str,
    label: &str,
    on_confirm: impl Fn() + 'static,
    on_dismiss: impl Fn() + 'static,
) {
    let on_dismiss = Rc::new(on_dismiss);
    let cancel = Rc::clone(&on_dismiss);
    dialog(
        move || on_dismiss(),
        || {
            column_with(Modifier::new().width(380.0), || {
                dialog_heading(heading, message);
                dialog_buttons(label, on_confirm, move || cancel());
            });
        },
    );
}

/// Shown instead of the app while signed out.
#[composable]
pub fn signed_out(ctx: &Ctx) {
    let theme = theme();
    stack_aligned(
        Modifier::new().fill_max_size().background(theme.background),
        BoxAlignment::Center,
        || {
            card(Modifier::new(), || {
                column_arranged(
                    Modifier::new().width(340.0).padding(28.0),
                    Arrangement::Start,
                    Alignment::Center,
                    || {
                        stack_aligned(
                            Modifier::new()
                                .size(40.0, 40.0)
                                .background_rounded(theme.primary, 10.0),
                            BoxAlignment::Center,
                            || icon(phosphor::STACK, 22.0, theme.on_primary),
                        );
                        spacer(Modifier::new().height(16.0));
                        title("You are signed out", 18.0);
                        spacer(Modifier::new().height(4.0));
                        muted_sized("Sign back in to see your dashboard.", 13.0);
                        spacer(Modifier::new().height(20.0));
                        let ctx = ctx.clone();
                        styled_button(
                            Modifier::new().fill_max_width(),
                            ButtonStyle::Default,
                            "Sign in as shadcn",
                            None,
                            None,
                            move || {
                                ctx.signed_in.set(true);
                                ctx.say("Welcome back");
                            },
                        );
                    },
                );
            });
        },
    );
}
