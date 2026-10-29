//! A dashboard in a restrained, monochrome style, built entirely from Rustle
//! components. Everything on it works: the sidebar navigates, tables sort and
//! filter, charts read out what the pointer is over, forms validate, and
//! dialogs, menus and sheets open over the page.

mod components;
mod ctx;
mod dashboard;
mod overlays;
mod overview;
mod pages;
mod shell;
mod ui;

use rustle::prelude::*;

use crate::ctx::{remember_ctx, toast_timer};

#[composable]
fn app() {
    let ctx = remember_ctx();
    let theme = match ctx.dark.get() {
        Some(true) => Theme::dark(),
        Some(false) => Theme::light(),
        None => system_theme(),
    };
    toast_timer(&ctx);

    provide_theme(theme, || {
        stack_with(Modifier::new().fill_max_size(), || {
            if ctx.signed_in.get() {
                shell(&ctx, theme);
            } else {
                overlays::signed_out(&ctx);
            }
            overlays::overlays(&ctx);
        });
    });
}

#[composable]
fn shell(ctx: &ctx::Ctx, theme: Theme) {
    row_arranged(
        Modifier::new().fill_max_size().background(theme.background),
        Arrangement::Start,
        Alignment::Start,
        || {
            shell::sidebar(ctx);
            column_with(Modifier::new().weight(1.0).fill_max_height(), || {
                shell::top_bar(ctx);
                let scroll = remember_scroll_state();
                // Each page starts at the top.
                key(ctx.page.get(), || {
                    column_with(
                        Modifier::new().fill_max_size().vertical_scroll(&scroll),
                        || page(ctx),
                    );
                });
            });
        },
    );
}

#[composable]
fn page(ctx: &ctx::Ctx) {
    match ctx.page.get() {
        0 => dashboard::dashboard_page(ctx),
        1 => overview::overview_page(ctx),
        2 => components::components_page(ctx),
        3 => pages::collection_page(
            ctx,
            0,
            "Projects",
            "Everything in flight across the team.",
            "project",
            Icon::Folder,
        ),
        4 => pages::team_page(ctx),
        5 => pages::collection_page(
            ctx,
            1,
            "Data Library",
            "Files your team has uploaded.",
            "file",
            Icon::File,
        ),
        6 => pages::collection_page(
            ctx,
            2,
            "Reports",
            "Generated summaries you can share.",
            "report",
            Icon::BarChart,
        ),
        7 => pages::assistant_page(ctx),
        8 => pages::settings_page(ctx),
        9 => pages::help_page(ctx),
        _ => pages::automations_page(ctx),
    }
}

fn main() {
    rustle::run(app);
}
