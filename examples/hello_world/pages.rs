//! The pages reached from the rest of the sidebar.

use rustle::prelude::*;

use crate::ctx::{Ctx, Overlay, ROLES};
use crate::overview::member_row;
use crate::ui::{chat, icon_action, panel};

#[composable]
fn page_frame(heading: &str, description: &str, content: impl FnOnce()) {
    column_with(Modifier::new().fill_max_width().padding(24.0), || {
        title(heading, 22.0);
        muted_sized(description, 14.0);
        spacer(Modifier::new().height(20.0));
        content();
    });
}

/// A searchable list that can be added to, starred and cleared out.
#[composable]
pub fn collection_page(
    ctx: &Ctx,
    index: usize,
    heading: &str,
    description: &str,
    noun: &str,
    glyph: Icon,
) {
    let theme = theme();
    let list = ctx.collections[index].clone();
    let query = remember_text_field_state("");
    let draft = remember_text_field_state("");
    let only_starred = remember(|| 0usize);
    let add = {
        let (list, draft, ctx, noun) = (list.clone(), draft.clone(), ctx.clone(), noun.to_string());
        move || {
            let name = draft.text();
            if name.trim().is_empty() {
                ctx.say(format!("Name the {noun} first"));
                return;
            }
            list.push(crate::ctx::Item {
                name: name.trim().to_string(),
                detail: "Added just now".to_string(),
                starred: false,
            });
            draft.clear();
            ctx.say(format!("Added {}", name.trim()));
        }
    };
    draft.on_submit(add.clone());
    page_frame(heading, description, || {
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpacedBy(8.0),
            Alignment::Center,
            || {
                stack_with(Modifier::new().weight(1.0), || {
                    text_field(Modifier::new().fill_max_width(), &query, "Search...");
                });
                let pick = only_starred.clone();
                segmented_control(only_starred.get(), &["All", "Starred"], move |i| {
                    pick.set(i)
                });
                stack_with(Modifier::new().width(220.0), || {
                    text_field(
                        Modifier::new().fill_max_width(),
                        &draft,
                        &format!("New {noun}"),
                    );
                });
                styled_button(
                    Modifier::new(),
                    ButtonStyle::Default,
                    &format!("Add {noun}"),
                    Some(Icon::Plus),
                    None,
                    add,
                );
            },
        );
        spacer(Modifier::new().height(16.0));
        let needle = query.text().to_lowercase();
        let shown: Vec<(usize, crate::ctx::Item)> = list
            .to_vec()
            .into_iter()
            .enumerate()
            .filter(|(_, item)| {
                item.name.to_lowercase().contains(&needle)
                    && (only_starred.get() == 0 || item.starred)
            })
            .collect();
        card(Modifier::new().fill_max_width(), || {
            column_with(Modifier::new().fill_max_width(), || {
                if shown.is_empty() {
                    stack_aligned(
                        Modifier::new().fill_max_width().padding(32.0),
                        BoxAlignment::Center,
                        || muted("Nothing here yet."),
                    );
                }
                for (position, (slot, item)) in shown.iter().enumerate() {
                    key(item.name.clone(), || {
                        let slot = *slot;
                        if position > 0 {
                            separator();
                        }
                        row_arranged(
                            Modifier::new().fill_max_width().padding_xy(16.0, 12.0),
                            Arrangement::SpaceBetween,
                            Alignment::Center,
                            || {
                                row_arranged(
                                    Modifier::new(),
                                    Arrangement::SpacedBy(14.0),
                                    Alignment::Center,
                                    || {
                                        let star = list.clone();
                                        icon_action(
                                            if item.starred {
                                                Icon::Check
                                            } else {
                                                Icon::Circle
                                            },
                                            if item.starred { "Unstar" } else { "Star" },
                                            move || {
                                                star.update(|l| l[slot].starred = !l[slot].starred)
                                            },
                                        );
                                        icon(glyph, 18.0, theme.on_surface_muted);
                                        column(|| {
                                            title(&item.name, 14.0);
                                            muted_sized(&item.detail, 13.0);
                                        });
                                    },
                                );
                                let items = vec![
                                    MenuItem::new("Duplicate", {
                                        let (list, item, ctx) =
                                            (list.clone(), item.clone(), ctx.clone());
                                        move || {
                                            list.push(crate::ctx::Item {
                                                name: format!("{} (copy)", item.name),
                                                ..item.clone()
                                            });
                                            ctx.say("Duplicated");
                                        }
                                    })
                                    .icon(Icon::Layers),
                                    MenuItem::new("Delete", {
                                        let (list, ctx) = (list.clone(), ctx.clone());
                                        move || {
                                            list.remove(slot);
                                            ctx.say("Deleted");
                                        }
                                    })
                                    .icon(Icon::X)
                                    .destructive(),
                                ];
                                menu(
                                    |open| {
                                        icon_action(Icon::MoreVertical, "Actions", move || open())
                                    },
                                    items,
                                );
                            },
                        );
                    });
                }
            });
        });
    });
}

#[composable]
pub fn team_page(ctx: &Ctx) {
    page_frame("Team", "Everyone who can see this workspace.", || {
        let invite = ctx.clone();
        styled_button(
            Modifier::new(),
            ButtonStyle::Default,
            "Invite member",
            Some(Icon::Plus),
            None,
            move || invite.open(Overlay::Invite),
        );
        spacer(Modifier::new().height(16.0));
        card(Modifier::new().fill_max_width(), || {
            column_with(Modifier::new().fill_max_width().padding(20.0), || {
                for (index, member) in ctx.members.to_vec().into_iter().enumerate() {
                    key(member.email.clone(), || {
                        row_arranged(
                            Modifier::new().fill_max_width(),
                            Arrangement::SpacedBy(8.0),
                            Alignment::Center,
                            || {
                                stack_with(Modifier::new().weight(1.0), || {
                                    member_row(ctx, index, &member);
                                });
                                let remove = ctx.clone();
                                icon_action(Icon::X, "Remove member", move || {
                                    remove.members.remove(index);
                                    remove.say(format!("Removed {}", member.name));
                                });
                            },
                        );
                    });
                }
                muted_sized(
                    &format!(
                        "{} members \u{2022} {} owner(s)",
                        ctx.members.len(),
                        ctx.members.to_vec().iter().filter(|m| m.role == 0).count()
                    ),
                    12.0,
                );
                let _ = ROLES;
            });
        });
    });
}

#[composable]
pub fn assistant_page(ctx: &Ctx) {
    let epoch = remember(|| 0usize);
    page_frame(
        "Word Assistant",
        "Ask for help drafting and editing.",
        || {
            card(Modifier::new().width_in(300.0, 640.0), || {
                column_with(Modifier::new().fill_max_width().padding(20.0), || {
                    row_arranged(
                        Modifier::new().fill_max_width(),
                        Arrangement::SpaceBetween,
                        Alignment::Center,
                        || {
                            title("Conversation", 14.0);
                            let (fresh, ctx) = (epoch.clone(), ctx.clone());
                            styled_button(
                                Modifier::new(),
                                ButtonStyle::Ghost,
                                "Clear",
                                None,
                                None,
                                move || {
                                    fresh.update(|e| *e += 1);
                                    ctx.say("Conversation cleared");
                                },
                            );
                        },
                    );
                    spacer(Modifier::new().height(12.0));
                    key(epoch.get(), || {
                        chat(
                            &[
                                "Here is a tighter version: lead with the result, then the reason.",
                                "I would split that into two sentences and cut the second adverb.",
                                "Sure. Want it formal or friendly?",
                            ],
                            &[("What would you like to write today?", false)],
                        );
                    });
                });
            });
        },
    );
}

#[composable]
pub fn settings_page(ctx: &Ctx) {
    let name = remember_text_field_state("shadcn");
    let digest = remember(|| true);
    let mentions = remember(|| true);
    page_frame("Settings", "Manage how the app looks and behaves.", || {
        column_with(Modifier::new().width_in(300.0, 560.0), || {
            panel("Profile", "This is how others see you.", || {
                text_field(Modifier::new().fill_max_width(), &name, "Display name");
                spacer(Modifier::new().height(12.0));
                let (ctx, name) = (ctx.clone(), name.clone());
                styled_button(
                    Modifier::new(),
                    ButtonStyle::Default,
                    "Save changes",
                    None,
                    None,
                    move || ctx.say(format!("Profile saved as {}", name.text())),
                );
            });
            panel("Appearance", "Pick a theme for the whole app.", || {
                let theme = ctx.dark.clone();
                select(
                    Modifier::new().fill_max_width(),
                    ["System", "Light", "Dark"].map(String::from).to_vec(),
                    match ctx.dark.get() {
                        None => 0,
                        Some(false) => 1,
                        Some(true) => 2,
                    },
                    move |i| theme.set([None, Some(false), Some(true)][i]),
                );
                spacer(Modifier::new().height(12.0));
                row_arranged(
                    Modifier::new().fill_max_width(),
                    Arrangement::SpaceBetween,
                    Alignment::Center,
                    || {
                        column(|| {
                            title("Compact sidebar", 14.0);
                            muted_sized("Show only icons in the sidebar.", 13.0);
                        });
                        let flip = ctx.collapsed.clone();
                        switch(ctx.collapsed.get(), move |v| flip.set(v));
                    },
                );
            });
            panel("Notifications", "Choose what you hear about.", || {
                for (label, detail, state) in [
                    ("Weekly digest", "A summary every Monday.", digest.clone()),
                    (
                        "Mentions",
                        "When someone comments on your work.",
                        mentions.clone(),
                    ),
                ] {
                    key(label, || {
                        row_arranged(
                            Modifier::new().fill_max_width().padding_xy(0.0, 8.0),
                            Arrangement::SpaceBetween,
                            Alignment::Center,
                            || {
                                column(|| {
                                    title(label, 14.0);
                                    muted_sized(detail, 13.0);
                                });
                                let flip = state.clone();
                                switch(state.get(), move |v| flip.set(v));
                            },
                        );
                    });
                }
            });
            panel("Session", "", || {
                row_arranged(
                    Modifier::new(),
                    Arrangement::SpacedBy(8.0),
                    Alignment::Center,
                    || {
                        let reset = ctx.clone();
                        styled_button(
                            Modifier::new(),
                            ButtonStyle::Outline,
                            "Mark notifications read",
                            None,
                            None,
                            move || {
                                reset
                                    .notices
                                    .update(|l| l.iter_mut().for_each(|(_, r)| *r = true));
                                reset.say("All notifications read");
                            },
                        );
                        let out = ctx.clone();
                        styled_button(
                            Modifier::new(),
                            ButtonStyle::Destructive,
                            "Log out",
                            None,
                            None,
                            move || out.open(Overlay::Logout),
                        );
                    },
                );
            });
        });
    });
}

#[composable]
pub fn help_page(ctx: &Ctx) {
    let theme = theme();
    let open = remember(|| None::<usize>);
    let message = remember_text_field_state("");
    page_frame(
        "Get Help",
        "Answers to common questions, and a way to reach us.",
        || {
            column_with(Modifier::new().width_in(300.0, 640.0), || {
                card(Modifier::new().fill_max_width(), || {
                    column_with(Modifier::new().fill_max_width().padding(8.0), || {
                        for (index, (question, answer)) in [
                        (
                            "How do I add a section to the outline?",
                            "Use Quick Create in the sidebar or Add Section above the table on the dashboard.",
                        ),
                        (
                            "Can I change the theme?",
                            "Yes. Settings has an appearance menu, or press Search and type theme.",
                        ),
                        (
                            "Where do my changes go?",
                            "Everything here is kept in memory for the session, except the page you were on.",
                        ),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        key(index, || {
                            let shown = open.get() == Some(index);
                            let toggle = open.clone();
                            column_with(
                                Modifier::new()
                                    .fill_max_width()
                                    .semantics(
                                        Semantics::new(Role::Button)
                                            .label(question.to_string())
                                            .merging_descendants(),
                                    )
                                    .clickable(move || {
                                        toggle.set(if shown { None } else { Some(index) })
                                    })
                                    .padding(12.0),
                                || {
                                    row_arranged(
                                        Modifier::new().fill_max_width(),
                                        Arrangement::SpaceBetween,
                                        Alignment::Center,
                                        || {
                                            title(question, 14.0);
                                            icon(
                                                if shown { Icon::ChevronUp } else { Icon::ChevronDown },
                                                14.0,
                                                theme.on_surface_muted,
                                            );
                                        },
                                    );
                                    if shown {
                                        spacer(Modifier::new().height(6.0));
                                        muted_sized(answer, 13.0);
                                    }
                                },
                            );
                            separator();
                        });
                    }
                    });
                });
                spacer(Modifier::new().height(16.0));
                panel("Contact support", "We reply within a day.", || {
                    text_area(
                        Modifier::new().fill_max_width().height(90.0),
                        &message,
                        "How can we help?",
                    );
                    spacer(Modifier::new().height(12.0));
                    row_arranged(
                        Modifier::new(),
                        Arrangement::SpacedBy(8.0),
                        Alignment::Center,
                        || {
                            let (ctx, message) = (ctx.clone(), message.clone());
                            styled_button(
                                Modifier::new(),
                                ButtonStyle::Default,
                                "Send",
                                Some(Icon::Send),
                                None,
                                move || {
                                    if message.text().trim().is_empty() {
                                        ctx.say("Write a message first");
                                    } else {
                                        message.clear();
                                        ctx.say("Message sent to support");
                                    }
                                },
                            );
                            styled_button(
                                Modifier::new(),
                                ButtonStyle::Outline,
                                "Documentation",
                                None,
                                Some(Icon::ArrowUpRight),
                                move || {
                                    let _ = rustle::open_url("https://docs.rs/");
                                },
                            );
                        },
                    );
                });
            });
        },
    );
}

#[composable]
pub fn automations_page(ctx: &Ctx) {
    let rules = remember_state_list(|| {
        vec![
            ("Assign new sections to a reviewer".to_string(), true),
            ("Email me when a section is done".to_string(), false),
            ("Archive finished projects after 30 days".to_string(), true),
        ]
    });
    page_frame("Automations", "Rules that run for you.", || {
        card(Modifier::new().width_in(300.0, 640.0), || {
            column_with(Modifier::new().fill_max_width().padding(20.0), || {
                for (index, (name, on)) in rules.to_vec().into_iter().enumerate() {
                    key(index, || {
                        row_arranged(
                            Modifier::new().fill_max_width().padding_xy(0.0, 8.0),
                            Arrangement::SpaceBetween,
                            Alignment::Center,
                            || {
                                title(&name, 14.0);
                                let (rules, ctx, name) = (rules.clone(), ctx.clone(), name.clone());
                                switch(on, move |value| {
                                    rules.set(index, (name.clone(), value));
                                    ctx.say(format!(
                                        "{} {}",
                                        name,
                                        if value { "enabled" } else { "paused" }
                                    ));
                                });
                            },
                        );
                    });
                }
            });
        });
    });
}
