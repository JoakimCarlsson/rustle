//! A dashboard in a restrained, monochrome style, built entirely from Rustle
//! components. Everything on it works: the sidebar navigates, tables sort and
//! filter, charts read out what the pointer is over, forms validate, and
//! dialogs, menus and sheets open over the page.

mod components;
mod ctx;
mod dashboard;
mod extended;
mod gallery;
mod motion;
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
    let scheme = match ctx.dark.get() {
        Some(true) => dark_color_scheme(),
        Some(false) => light_color_scheme(),
        None if is_system_in_dark_theme() => dark_color_scheme(),
        None => light_color_scheme(),
    };
    toast_timer(&ctx);

    let extended = if scheme == dark_color_scheme() {
        extended::ExtendedColors::dark()
    } else {
        extended::ExtendedColors::light()
    };
    extended::provide_extended_colors(extended, || {
        theme(scheme, None, None, || {
            stack_with(Modifier::new().fill_max_size(), || {
                if ctx.signed_in.get() {
                    shell(&ctx, scheme);
                } else {
                    overlays::signed_out(&ctx);
                }
                overlays::overlays(&ctx);
            });
        })
    });
}

#[composable]
fn shell(ctx: &ctx::Ctx, theme: ColorScheme) {
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
            phosphor::FOLDER,
        ),
        4 => pages::team_page(ctx),
        5 => pages::collection_page(
            ctx,
            1,
            "Data Library",
            "Files your team has uploaded.",
            "file",
            phosphor::FILE,
        ),
        6 => pages::collection_page(
            ctx,
            2,
            "Reports",
            "Generated summaries you can share.",
            "report",
            phosphor::CHART_BAR,
        ),
        7 => pages::assistant_page(ctx),
        8 => pages::settings_page(ctx),
        9 => pages::help_page(ctx),
        11 => gallery::gallery_page(ctx),
        12 => motion::motion_page(ctx),
        _ => pages::automations_page(ctx),
    }
}

fn main() {
    rustle::run(app);
}
