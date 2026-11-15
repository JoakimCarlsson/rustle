//! The command palette.

use std::rc::Rc;

use rustle::prelude::*;

use crate::ctx::{Ctx, Overlay, PAGES};

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
            glyph: phosphor::ARROW_RIGHT,
            run: Rc::new(move || ctx.go(index)),
        });
    }
    let c = ctx.clone();
    list.push(Command {
        label: "Create document".into(),
        glyph: phosphor::PLUS,
        run: Rc::new(move || c.open(Overlay::NewDocument)),
    });
    let c = ctx.clone();
    list.push(Command {
        label: "Toggle sidebar".into(),
        glyph: phosphor::SIDEBAR_SIMPLE,
        run: Rc::new(move || c.collapsed.update(|v| *v = !*v)),
    });
    let c = ctx.clone();
    list.push(Command {
        label: "Switch to light theme".into(),
        glyph: phosphor::CIRCLE,
        run: Rc::new(move || c.dark.set(Some(false))),
    });
    let c = ctx.clone();
    list.push(Command {
        label: "Switch to dark theme".into(),
        glyph: phosphor::CIRCLE,
        run: Rc::new(move || c.dark.set(Some(true))),
    });
    let c = ctx.clone();
    list.push(Command {
        label: "Open GitHub".into(),
        glyph: phosphor::ARROW_UP_RIGHT,
        run: Rc::new(move || {
            rustle::open_url("https://github.com/");
            c.say("Opening GitHub in your browser");
        }),
    });
    let c = ctx.clone();
    list.push(Command {
        label: "Log out".into(),
        glyph: phosphor::LOCK,
        run: Rc::new(move || c.open(Overlay::Logout)),
    });
    list
}

#[composable]
pub(super) fn search_palette(ctx: &Ctx) {
    let theme = color_scheme();
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
                                                        theme.surface_variant
                                                    } else {
                                                        Color::TRANSPARENT
                                                    },
                                                    8.0,
                                                )
                                                .padding_xy(10.0, 9.0),
                                            Arrangement::SpacedBy(10.0),
                                            Alignment::Center,
                                            || {
                                                icon(
                                                    command.glyph.clone(),
                                                    15.0,
                                                    theme.on_surface_variant,
                                                );
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
