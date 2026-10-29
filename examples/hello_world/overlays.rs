//! Everything that opens over a page: dialogs, sheets and the toast.

use std::rc::Rc;

use rustle::prelude::*;

use crate::components::ACCOUNTS;
use crate::ctx::{Ctx, KINDS, Overlay, PAGES, REVIEWERS};
use crate::ui::{icon_box, key_value, money};

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
                    icon(Icon::Check, 15.0, theme.success);
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

struct Command {
    label: String,
    glyph: Icon,
    run: Rc<dyn Fn()>,
}

fn commands(ctx: &Ctx) -> Vec<Command> {
    let mut list = Vec::new();
    for (index, name) in PAGES.iter().enumerate() {
        let ctx = ctx.clone();
        list.push(Command {
            label: format!("Go to {name}"),
            glyph: Icon::ArrowRight,
            run: Rc::new(move || ctx.go(index)),
        });
    }
    let c = ctx.clone();
    list.push(Command {
        label: "Create document".into(),
        glyph: Icon::Plus,
        run: Rc::new(move || c.open(Overlay::NewDocument)),
    });
    let c = ctx.clone();
    list.push(Command {
        label: "Toggle sidebar".into(),
        glyph: Icon::PanelLeft,
        run: Rc::new(move || c.collapsed.update(|v| *v = !*v)),
    });
    let c = ctx.clone();
    list.push(Command {
        label: "Switch to light theme".into(),
        glyph: Icon::Circle,
        run: Rc::new(move || c.dark.set(Some(false))),
    });
    let c = ctx.clone();
    list.push(Command {
        label: "Switch to dark theme".into(),
        glyph: Icon::Circle,
        run: Rc::new(move || c.dark.set(Some(true))),
    });
    let c = ctx.clone();
    list.push(Command {
        label: "Open GitHub".into(),
        glyph: Icon::ArrowUpRight,
        run: Rc::new(move || {
            rustle::open_url("https://github.com/");
            c.say("Opening GitHub in your browser");
        }),
    });
    let c = ctx.clone();
    list.push(Command {
        label: "Log out".into(),
        glyph: Icon::Lock,
        run: Rc::new(move || c.open(Overlay::Logout)),
    });
    list
}

#[composable]
fn search_palette(ctx: &Ctx) {
    let theme = theme();
    let query = remember_text_field_state("");
    let focus = remember_focus();
    let asked = remember(|| false);
    if !asked.get() {
        asked.set(true);
        focus.request_focus();
    }
    let needle = query.text().to_lowercase();
    let matches: Vec<Command> = commands(ctx)
        .into_iter()
        .filter(|command| command.label.to_lowercase().contains(&needle))
        .collect();
    let run = |command: &Rc<dyn Fn()>, ctx: &Ctx| {
        ctx.close();
        command();
    };
    if let Some(first) = matches.first() {
        let (first, ctx) = (Rc::clone(&first.run), ctx.clone());
        query.on_submit(move || run(&first, &ctx));
    }
    dialog(
        {
            let ctx = ctx.clone();
            move || ctx.close()
        },
        || {
            column_with(Modifier::new().width(440.0), || {
                text_field_focused(
                    Modifier::new().fill_max_width(),
                    &query,
                    "Type a command or search...",
                    &focus,
                );
                spacer(Modifier::new().height(10.0));
                if matches.is_empty() {
                    stack_with(Modifier::new().padding(12.0), || muted("No results found."));
                }
                column_with(
                    Modifier::new().fill_max_width().height_in(0.0, 320.0),
                    || {
                        let scroll = remember_scroll_state();
                        column_with(
                            Modifier::new().fill_max_width().vertical_scroll(&scroll),
                            || {
                                for (index, command) in matches.iter().enumerate() {
                                    key(index, || {
                                        let (go, ctx) = (Rc::clone(&command.run), ctx.clone());
                                        row_arranged(
                                            Modifier::new()
                                                .fill_max_width()
                                                .semantics(
                                                    Semantics::new(Role::Button)
                                                        .label(command.label.clone())
                                                        .merging_descendants(),
                                                )
                                                .clickable(move || run(&go, &ctx))
                                                .background_rounded(
                                                    if index == 0 {
                                                        theme.accent
                                                    } else {
                                                        Color::TRANSPARENT
                                                    },
                                                    8.0,
                                                )
                                                .padding_xy(10.0, 9.0),
                                            Arrangement::SpacedBy(10.0),
                                            Alignment::Center,
                                            || {
                                                icon(command.glyph, 15.0, theme.on_surface_muted);
                                                text(command.label.clone());
                                            },
                                        );
                                    });
                                }
                            },
                        );
                    },
                );
            });
        },
    );
}

#[composable]
fn new_document(ctx: &Ctx) {
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
fn invite(ctx: &Ctx) {
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
fn pending_setup(ctx: &Ctx) {
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
                    icon(Icon::ChevronRight, 14.0, theme.on_surface_muted);
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

#[composable]
fn report_sheet(ctx: &Ctx) {
    let theme = theme();
    column_with(Modifier::new().fill_max_width().padding(24.0), || {
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceBetween,
            Alignment::Center,
            || {
                column(|| {
                    title("Full report", 18.0);
                    muted_sized("Contributions from December to May.", 13.0);
                });
                let ctx = ctx.clone();
                icon_box(Icon::X, 30.0, "Close", move || ctx.close());
            },
        );
        spacer(Modifier::new().height(16.0));
        for (month, amount) in [
            ("December", 5200.0),
            ("January", 7400.0),
            ("February", 6600.0),
            ("March", 8200.0),
            ("April", 5600.0),
            ("May", 7800.0),
        ] {
            key(month, || key_value(month, &money(amount), false));
        }
        separator();
        key_value("Total", &money(40_800.0), true);
        spacer(Modifier::new().height(16.0));
        row_arranged(
            Modifier::new(),
            Arrangement::SpacedBy(8.0),
            Alignment::Center,
            || {
                let save = ctx.clone();
                styled_button(
                    Modifier::new(),
                    ButtonStyle::Default,
                    "Download PDF",
                    Some(Icon::File),
                    None,
                    move || {
                        save.close();
                        save.say("Report saved as contributions.pdf");
                    },
                );
                let ctx = ctx.clone();
                styled_button(
                    Modifier::new(),
                    ButtonStyle::Outline,
                    "Email me a copy",
                    Some(Icon::Mail),
                    None,
                    move || {
                        ctx.close();
                        ctx.say("Report sent to m@example.com");
                    },
                );
            },
        );
        let _ = theme;
    });
}

#[composable]
fn detail_sheet(ctx: &Ctx, id: usize) {
    let Some(doc) = ctx.doc(id) else {
        return;
    };
    column_with(Modifier::new().fill_max_width().padding(24.0), || {
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceBetween,
            Alignment::Center,
            || {
                column(|| {
                    title(&doc.header, 18.0);
                    muted_sized(
                        &format!("{} \u{2022} reviewed by {}", doc.kind, doc.reviewer),
                        13.0,
                    );
                });
                let ctx = ctx.clone();
                icon_box(Icon::X, 30.0, "Close", move || ctx.close());
            },
        );
        spacer(Modifier::new().height(16.0));
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpacedBy(16.0),
            Alignment::Start,
            || {
                for (label, value, field) in [("Target", doc.target, 0), ("Limit", doc.limit, 1)] {
                    key(field, || {
                        column_with(Modifier::new().weight(1.0), || {
                            muted_sized(label, 12.0);
                            spacer(Modifier::new().height(6.0));
                            row_arranged(
                                Modifier::new(),
                                Arrangement::SpacedBy(12.0),
                                Alignment::Center,
                                || {
                                    let down = ctx.clone();
                                    icon_box(Icon::ChevronLeft, 30.0, "Decrease", move || {
                                        down.change_doc(id, |d| {
                                            let v = if field == 0 {
                                                &mut d.target
                                            } else {
                                                &mut d.limit
                                            };
                                            *v = v.saturating_sub(1);
                                        })
                                    });
                                    title(&value.to_string(), 20.0);
                                    let up = ctx.clone();
                                    icon_box(Icon::ChevronRight, 30.0, "Increase", move || {
                                        up.change_doc(id, |d| {
                                            let v = if field == 0 {
                                                &mut d.target
                                            } else {
                                                &mut d.limit
                                            };
                                            *v += 1;
                                        })
                                    });
                                },
                            );
                        });
                    });
                }
                column_with(Modifier::new().weight(1.0), || {
                    muted_sized("Reviewer", 12.0);
                    spacer(Modifier::new().height(6.0));
                    let pick = ctx.clone();
                    select(
                        Modifier::new().fill_max_width(),
                        REVIEWERS.iter().map(|r| r.to_string()).collect(),
                        REVIEWERS
                            .iter()
                            .position(|r| *r == doc.reviewer)
                            .unwrap_or(0),
                        move |index| {
                            pick.change_doc(id, |d| d.reviewer = REVIEWERS[index].to_string())
                        },
                    );
                });
            },
        );
        spacer(Modifier::new().height(20.0));
        row_arranged(
            Modifier::new(),
            Arrangement::SpacedBy(8.0),
            Alignment::Center,
            || {
                let toggle = ctx.clone();
                styled_button(
                    Modifier::new(),
                    ButtonStyle::Default,
                    if doc.done { "Reopen" } else { "Mark as done" },
                    Some(Icon::Check),
                    None,
                    move || {
                        toggle.change_doc(id, |d| d.done = !d.done);
                        toggle.say("Status updated");
                    },
                );
                let remove = ctx.clone();
                styled_button(
                    Modifier::new(),
                    ButtonStyle::Destructive,
                    "Delete",
                    None,
                    None,
                    move || {
                        remove.close();
                        remove.delete_docs(&[id]);
                        remove.say("Section deleted");
                    },
                );
            },
        );
    });
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
                            || icon(Icon::Layers, 22.0, theme.on_primary),
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
