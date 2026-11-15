//! The frame around every page: the sidebar and the top bar.

use rustle::prelude::*;

use crate::ctx::{Ctx, Overlay, PAGES};
use crate::ui::icon_action;

#[composable]
pub fn sidebar(ctx: &Ctx) {
    let theme = color_scheme();
    let compact = ctx.collapsed.get();
    let page = ctx.page.get();
    let width = animate_float_as_state(if compact { 64.0 } else { 256.0 }, tween(180)).get();
    column_with(
        Modifier::new()
            .width(width)
            .fill_max_height()
            .clip()
            .border_right_hairline(theme.outline)
            .padding(12.0),
        || {
            row_arranged(
                Modifier::new().padding_xy(8.0, 6.0).clickable({
                    let ctx = ctx.clone();
                    move || ctx.go(0)
                }),
                Arrangement::SpacedBy(10.0),
                Alignment::Center,
                || {
                    stack_aligned(
                        Modifier::new()
                            .size(24.0, 24.0)
                            .background_rounded(theme.primary, 6.0),
                        BoxAlignment::Center,
                        || icon(phosphor::STACK, 14.0, theme.on_primary),
                    );
                    if !compact {
                        title("Acme Inc.", 15.0);
                    }
                },
            );
            spacer(Modifier::new().height(12.0));
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpacedBy(8.0),
                Alignment::Center,
                || {
                    let create = ctx.clone();
                    if compact {
                        icon_action(phosphor::PLUS, "Quick Create", move || {
                            create.open(Overlay::NewDocument)
                        });
                    } else {
                        styled_button(
                            Modifier::new().weight(1.0),
                            ButtonStyle::Default,
                            "Quick Create",
                            Some(phosphor::PLUS),
                            None,
                            move || create.open(Overlay::NewDocument),
                        );
                        notifications(ctx);
                    }
                },
            );
            spacer(Modifier::new().height(16.0));

            for (label, glyph, target) in [
                ("Dashboard", phosphor::SQUARES_FOUR, 0),
                ("Overview", phosphor::PULSE, 1),
                ("Components", phosphor::STACK, 2),
                ("Projects", phosphor::FOLDER, 3),
                ("Team", phosphor::USERS, 4),
            ] {
                key(target, || {
                    let go = ctx.clone();
                    nav_item(glyph, label, page == target, compact, move || go.go(target));
                });
            }

            spacer(Modifier::new().height(20.0));
            if !compact {
                stack_with(Modifier::new().padding_xy(10.0, 4.0), || {
                    muted_sized("Showcase", 12.0);
                });
            }
            for (label, glyph, target) in [
                ("Gallery", phosphor::DOTS_SIX_VERTICAL, 11),
                ("Motion", phosphor::PULSE, 12),
            ] {
                key(target, || {
                    let go = ctx.clone();
                    nav_item(glyph, label, page == target, compact, move || go.go(target));
                });
            }

            spacer(Modifier::new().height(20.0));
            if !compact {
                stack_with(Modifier::new().padding_xy(10.0, 4.0), || {
                    muted_sized("Documents", 12.0);
                });
            }
            for (label, glyph, target) in [
                ("Data Library", phosphor::STACK, 5),
                ("Reports", phosphor::FILE, 6),
                ("Word Assistant", phosphor::PAPER_PLANE_TILT, 7),
            ] {
                key(target, || {
                    let go = ctx.clone();
                    nav_item(glyph, label, page == target, compact, move || go.go(target));
                });
            }
            let more = ctx.more.clone();
            if ctx.more.get() {
                let go = ctx.clone();
                nav_item(
                    phosphor::PULSE,
                    "Automations",
                    page == 10,
                    compact,
                    move || go.go(10),
                );
            }
            nav_item(
                if ctx.more.get() {
                    phosphor::CARET_UP
                } else {
                    phosphor::DOTS_THREE
                },
                if ctx.more.get() { "Less" } else { "More" },
                false,
                compact,
                move || more.update(|open| *open = !*open),
            );

            spacer(Modifier::new().weight(1.0));
            let go = ctx.clone();
            nav_item(phosphor::GEAR, "Settings", page == 8, compact, move || {
                go.go(8)
            });
            let go = ctx.clone();
            nav_item(
                phosphor::QUESTION,
                "Get Help",
                page == 9,
                compact,
                move || go.go(9),
            );
            let search = ctx.clone();
            nav_item(
                phosphor::MAGNIFYING_GLASS,
                "Search",
                false,
                compact,
                move || search.open(Overlay::Search),
            );
            spacer(Modifier::new().height(8.0));
            user_row(ctx, compact);
        },
    );
}

/// The mail button, which opens the notifications.
#[composable]
fn notifications(ctx: &Ctx) {
    let theme = color_scheme();
    let unread = ctx.unread();
    let mut items: Vec<MenuItem> = ctx
        .notices
        .to_vec()
        .into_iter()
        .enumerate()
        .map(|(index, (message, read))| {
            let ctx = ctx.clone();
            MenuItem::new(
                if read {
                    message.clone()
                } else {
                    format!("\u{2022} {message}")
                },
                move || {
                    ctx.notices.set(index, (message.clone(), true));
                    ctx.say("Marked as read");
                },
            )
        })
        .collect();
    let all = ctx.clone();
    items.push(
        MenuItem::new("Mark all as read", move || {
            all.notices
                .update(|list| list.iter_mut().for_each(|(_, read)| *read = true));
            all.say("All notifications read");
        })
        .icon(phosphor::CHECK),
    );
    menu(
        |open| {
            stack_aligned(
                Modifier::new()
                    .size(34.0, 34.0)
                    .semantics(Semantics::new(Role::Button).label("Notifications".to_string()))
                    .clickable(move || open())
                    .border(1.0, theme.outline, 8.0),
                BoxAlignment::Center,
                || {
                    icon(phosphor::ENVELOPE_SIMPLE, 15.0, theme.on_surface);
                    if unread > 0 {
                        stack_aligned(
                            Modifier::new().size(34.0, 34.0).padding(5.0),
                            BoxAlignment::TopEnd,
                            || {
                                spacer(
                                    Modifier::new()
                                        .size(8.0, 8.0)
                                        .background_rounded(theme.error, 4.0),
                                )
                            },
                        );
                    }
                },
            );
        },
        items,
    );
}

#[composable]
fn user_row(ctx: &Ctx, compact: bool) {
    let theme = color_scheme();
    let items = vec![
        MenuItem::new("Account", {
            let ctx = ctx.clone();
            move || ctx.go(8)
        })
        .icon(phosphor::USER),
        MenuItem::new("Billing", {
            let ctx = ctx.clone();
            move || {
                ctx.go(2);
                ctx.say("Billing is on the Components page");
            }
        })
        .icon(phosphor::CREDIT_CARD),
        MenuItem::new("Notifications", {
            let ctx = ctx.clone();
            move || ctx.go(8)
        })
        .icon(phosphor::BELL),
        MenuItem::new("Log out", {
            let ctx = ctx.clone();
            move || ctx.open(Overlay::Logout)
        })
        .icon(phosphor::LOCK)
        .destructive(),
    ];
    menu(
        |open| {
            row_arranged(
                Modifier::new()
                    .fill_max_width()
                    .semantics(
                        Semantics::new(Role::Button)
                            .label("Account menu".to_string())
                            .merging_descendants(),
                    )
                    .clickable(move || open())
                    .padding_xy(8.0, 6.0),
                Arrangement::SpaceBetween,
                Alignment::Center,
                || {
                    row_arranged(
                        Modifier::new(),
                        Arrangement::SpacedBy(10.0),
                        Alignment::Center,
                        || {
                            avatar("SH", 32.0);
                            if !compact {
                                column(|| {
                                    title("shadcn", 13.0);
                                    muted_sized("m@example.com", 12.0);
                                });
                            }
                        },
                    );
                    if !compact {
                        icon(
                            phosphor::DOTS_THREE_VERTICAL,
                            16.0,
                            theme.on_surface_variant,
                        );
                    }
                },
            );
        },
        items,
    );
}

#[composable]
fn nav_item(glyph: Icon, label: &str, active: bool, compact: bool, on_click: impl Fn() + 'static) {
    let theme = color_scheme();
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
            .clickable(on_click)
            .background_rounded(
                if active {
                    theme.surface_variant
                } else {
                    Color::TRANSPARENT
                },
                8.0,
            )
            .padding_xy(10.0, 8.0),
        Arrangement::SpacedBy(10.0),
        Alignment::Center,
        || {
            icon(glyph, 16.0, color);
            if !compact {
                let style = LOCAL_TEXT_STYLE
                    .current()
                    .color(color)
                    .font_weight(FontWeight::MEDIUM)
                    .max_lines(1);
                text_styled(Modifier::new(), label.to_string(), style);
            }
        },
    );
}

#[composable]
pub fn top_bar(ctx: &Ctx) {
    let theme = color_scheme();
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .height(52.0)
            .border_bottom_hairline(theme.outline)
            .padding_xy(20.0, 0.0),
        Arrangement::SpaceBetween,
        Alignment::Center,
        || {
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(12.0),
                Alignment::Center,
                || {
                    let collapse = ctx.collapsed.clone();
                    icon_action(phosphor::SIDEBAR_SIMPLE, "Toggle sidebar", move || {
                        collapse.update(|c| *c = !*c)
                    });
                    spacer(Modifier::new().size(1.0, 16.0).background(theme.outline));
                    title(PAGES[ctx.page.get().min(PAGES.len() - 1)], 14.0);
                },
            );
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(4.0),
                Alignment::Center,
                || {
                    let search = ctx.clone();
                    icon_action(phosphor::MAGNIFYING_GLASS, "Search", move || {
                        search.open(Overlay::Search)
                    });
                    let ctx = ctx.clone();
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Ghost,
                        "GitHub",
                        None,
                        Some(phosphor::ARROW_UP_RIGHT),
                        move || {
                            rustle::open_url("https://github.com/");
                            ctx.say("Opening GitHub in your browser");
                        },
                    );
                },
            );
        },
    );
}
