//! Lazy lists and grids of items of different sizes: placement animation,
//! horizontal lists, staggered and spanned grids and sticky headers.

use rustle::prelude::*;

use crate::ui::panel;

/// A tile of `label` in the color picked by `seed`.
#[composable]
fn tile(label: &str, seed: usize, modifier: Modifier) {
    let theme = color_scheme();
    let shade = 40 + (seed * 37 % 60) as u8;
    stack_aligned(
        modifier
            .background_rounded(Color::rgb(shade, shade + 20, shade + 60), 10.0)
            .border(1.0, theme.outline, 10.0),
        BoxAlignment::Center,
        || title(label, 13.0),
    );
}

/// A list whose items can be shuffled, inserted and removed, sliding to their places.
#[composable]
fn reorder_list() {
    let ids = remember(|| (0..8_u64).collect::<Vec<_>>());
    let next = remember(|| 8_u64);
    let state = remember_lazy_list_state();
    panel(
        "Animated placement",
        "Keyed items slide when the list is reordered, or when items are inserted or removed.",
        || {
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(8.0),
                Alignment::Start,
                || {
                    let shuffled = ids.clone();
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Outline,
                        "Reverse",
                        None,
                        None,
                        move || {
                            shuffled.update(|ids| ids.reverse());
                        },
                    );
                    let inserted = ids.clone();
                    let counter = next.clone();
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Outline,
                        "Insert",
                        None,
                        None,
                        move || {
                            let id = counter.get();
                            counter.set(id + 1);
                            inserted.update(|ids| ids.insert(1, id));
                        },
                    );
                    let removed = ids.clone();
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Outline,
                        "Remove",
                        None,
                        None,
                        move || {
                            removed.update(|ids| {
                                if ids.len() > 1 {
                                    ids.remove(1);
                                }
                            });
                        },
                    );
                },
            );
            spacer(Modifier::new().height(8.0));
            let current = ids.get();
            lazy_list_with(
                Modifier::new()
                    .fill_max_width()
                    .height(220.0)
                    .clip_rounded(10.0),
                &state,
                LazyContent::new()
                    .padding(6.0)
                    .spacing(6.0)
                    .animate_item(spring()),
                current.len(),
                |index| current[index],
                |index| {
                    let id = current[index];
                    tile(
                        &format!("Item {id}"),
                        id as usize,
                        Modifier::new()
                            .fill_max_width()
                            .height(28.0 + (id % 3) as f32 * 14.0),
                    );
                },
            );
        },
    );
}

/// A horizontal list of items of different widths.
#[composable]
fn horizontal_list() {
    let state = remember_lazy_list_state();
    panel(
        "Horizontal list",
        "Widths are learned as items are laid out.",
        || {
            lazy_horizontal_list_with(
                Modifier::new()
                    .fill_max_width()
                    .height(64.0)
                    .clip_rounded(10.0),
                &state,
                LazyContent::new().padding(6.0).spacing(6.0),
                200,
                |index| index,
                |index| {
                    tile(
                        &format!("Card {index}"),
                        index,
                        Modifier::new()
                            .width(70.0 + (index * 13 % 70) as f32)
                            .fill_max_height(),
                    );
                },
            );
        },
    );
}

/// Staggered grids in both directions and a grid with spans.
#[composable]
fn grids() {
    let vertical = remember_lazy_list_state();
    let horizontal = remember_lazy_list_state();
    let spanned = remember_lazy_list_state();
    panel(
        "Staggered and spanned grids",
        "Items go to the shortest lane; spanned items share rows of equal columns.",
        || {
            lazy_vertical_staggered_grid(
                Modifier::new()
                    .fill_max_width()
                    .height(220.0)
                    .clip_rounded(10.0),
                &vertical,
                LazyContent::new().padding(6.0).spacing(6.0),
                3,
                300,
                |index| index,
                |index| {
                    tile(
                        &format!("{index}"),
                        index,
                        Modifier::new()
                            .fill_max_width()
                            .height(40.0 + (index * 29 % 80) as f32),
                    );
                },
            );
            spacer(Modifier::new().height(12.0));
            lazy_horizontal_staggered_grid(
                Modifier::new()
                    .fill_max_width()
                    .height(140.0)
                    .clip_rounded(10.0),
                &horizontal,
                LazyContent::new().padding(6.0).spacing(6.0),
                2,
                300,
                |index| index,
                |index| {
                    tile(
                        &format!("{index}"),
                        index,
                        Modifier::new()
                            .fill_max_height()
                            .width(50.0 + (index * 31 % 90) as f32),
                    );
                },
            );
            spacer(Modifier::new().height(12.0));
            lazy_vertical_grid_spanned(
                Modifier::new()
                    .fill_max_width()
                    .height(220.0)
                    .clip_rounded(10.0),
                &spanned,
                LazyContent::new().padding(6.0).spacing(6.0),
                4,
                300,
                |index| index,
                |index| 1 + index % 3,
                |index| {
                    tile(
                        &format!("{index}"),
                        index,
                        Modifier::new()
                            .fill_max_width()
                            .height(36.0 + (index % 4) as f32 * 14.0),
                    );
                },
            );
        },
    );
}

/// A list of sections whose headers stay at the top.
#[composable]
fn sticky_headers() {
    let state = remember_lazy_list_state();
    panel(
        "Sticky headers",
        "The header of the section in view stays put until the next one pushes it away.",
        || {
            lazy_sticky_list(
                Modifier::new()
                    .fill_max_width()
                    .height(220.0)
                    .clip_rounded(10.0),
                &state,
                LazyContent::new().spacing(4.0),
                120,
                |index| index,
                |index| index % 6 == 0,
                |index| {
                    if index % 6 == 0 {
                        stack_with(
                            Modifier::new()
                                .fill_max_width()
                                .background(color_scheme().surface_variant)
                                .padding_xy(12.0, 8.0),
                            || title(&format!("Section {}", index / 6 + 1), 13.0),
                        );
                    } else {
                        stack_with(
                            Modifier::new().fill_max_width().padding_xy(12.0, 6.0),
                            || muted_sized(&format!("Entry {index}"), 13.0),
                        );
                    }
                },
            );
        },
    );
}

/// The lazy lists section of the motion page.
#[composable]
pub(super) fn lazy_lists() {
    reorder_list();
    horizontal_list();
    sticky_headers();
    grids();
}
