//! The metrics page: stat cards that pick what the chart shows, and a
//! document table that filters, sorts, selects and pages.

use rustle::prelude::*;

use crate::ctx::{Ctx, Doc, Overlay};
use crate::ui::{chart_style, icon_action, icon_box, wander};

struct Metric {
    label: &'static str,
    value: &'static str,
    delta: &'static str,
    up: bool,
    headline: &'static str,
    note: &'static str,
    names: (&'static str, &'static str),
    seed: f32,
    base: f32,
}

const METRICS: [Metric; 4] = [
    Metric {
        label: "Total Revenue",
        value: "$1,250.00",
        delta: "+12.5%",
        up: true,
        headline: "Trending up this month",
        note: "Visitors for the last 6 months",
        names: ("Mobile", "Desktop"),
        seed: 1.0,
        base: 230.0,
    },
    Metric {
        label: "New Customers",
        value: "1,234",
        delta: "-20%",
        up: false,
        headline: "Down 20% this period",
        note: "Acquisition needs attention",
        names: ("Referral", "Organic"),
        seed: 4.2,
        base: 140.0,
    },
    Metric {
        label: "Active Accounts",
        value: "45,678",
        delta: "+12.5%",
        up: true,
        headline: "Strong user retention",
        note: "Engagement exceed targets",
        names: ("Free", "Paid"),
        seed: 7.7,
        base: 300.0,
    },
    Metric {
        label: "Growth Rate",
        value: "4.5%",
        delta: "+4.5%",
        up: true,
        headline: "Steady performance increase",
        note: "Meets growth projections",
        names: ("Target", "Actual"),
        seed: 9.1,
        base: 90.0,
    },
];

#[composable]
fn delta_pill(label: &str, up: bool) {
    let theme = theme();
    row_arranged(
        Modifier::new()
            .border(1.0, theme.outline, 100.0)
            .padding_xy(8.0, 2.0),
        Arrangement::SpacedBy(4.0),
        Alignment::Center,
        || {
            icon(
                if up {
                    Icon::TrendingUp
                } else {
                    Icon::TrendingDown
                },
                12.0,
                theme.on_surface,
            );
            let style = LOCAL_TEXT_STYLE
                .current()
                .font_size(12.0)
                .font_weight(FontWeight::MEDIUM);
            text_styled(Modifier::new(), label.to_string(), style);
        },
    );
}

#[composable]
fn stat_card(metric: &Metric, selected: bool, on_click: impl Fn() + 'static) {
    let theme = theme();
    card(
        Modifier::new()
            .weight(1.0)
            .semantics(
                Semantics::new(Role::Button)
                    .label(metric.label.to_string())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .border(
                1.5,
                if selected {
                    theme.on_surface_muted
                } else {
                    Color::TRANSPARENT
                },
                theme.corner_radius,
            ),
        || {
            column_with(Modifier::new().fill_max_width().padding(20.0), || {
                row_arranged(
                    Modifier::new().fill_max_width(),
                    Arrangement::SpaceBetween,
                    Alignment::Center,
                    || {
                        muted(metric.label);
                        delta_pill(metric.delta, metric.up);
                    },
                );
                spacer(Modifier::new().height(8.0));
                title(metric.value, 28.0);
                spacer(Modifier::new().height(14.0));
                row_arranged(
                    Modifier::new(),
                    Arrangement::SpacedBy(6.0),
                    Alignment::Center,
                    || {
                        title(metric.headline, 13.0);
                        icon(
                            if metric.up {
                                Icon::TrendingUp
                            } else {
                                Icon::TrendingDown
                            },
                            14.0,
                            theme.on_surface,
                        );
                    },
                );
                muted_sized(metric.note, 13.0);
            });
        },
    );
}

fn day_label(offset: usize) -> String {
    if offset < 30 {
        format!("Apr {}", offset + 1)
    } else if offset < 61 {
        format!("May {}", offset - 29)
    } else {
        format!("Jun {}", offset - 60)
    }
}

#[composable]
pub fn dashboard_page(ctx: &Ctx) {
    let metric = remember(|| 0usize);
    column_with(Modifier::new().fill_max_width().padding(24.0), || {
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpacedBy(16.0),
            Alignment::Start,
            || {
                for (index, item) in METRICS.iter().enumerate() {
                    key(index, || {
                        let pick = metric.clone();
                        stat_card(item, metric.get() == index, move || pick.set(index));
                    });
                }
            },
        );
        spacer(Modifier::new().height(16.0));
        visitors_chart(&METRICS[metric.get().min(3)]);
        spacer(Modifier::new().height(16.0));
        document_section(ctx);
    });
}

#[composable]
fn visitors_chart(metric: &Metric) {
    let theme = theme();
    let range = remember(|| 0usize);
    // April 7 to June 30 is 85 days; shorter ranges end on the same day.
    let points = [85usize, 30, 7][range.get().min(2)];
    let first = wander(points, metric.seed, metric.base, 160.0);
    let second = wander(points, metric.seed + 1.4, metric.base + 100.0, 190.0);
    let labels: Vec<String> = (0..points)
        .map(|i| day_label(6 + (85 - points) + i))
        .collect();
    let (hover, hovered) = chart_hover(points, 0.0, 0.0);
    let width = remember(|| std::rc::Rc::new(std::cell::Cell::new(1000.0_f32))).get();
    let measure = std::rc::Rc::clone(&width);
    let hover = hover.on_size_changed(move |size| measure.set(size.width));
    let marker = hovered.map(|i| i.min(points - 1));
    let readout = marker.map(|m| {
        (
            labels[m].clone(),
            first[m].round() as i32,
            second[m].round() as i32,
        )
    });
    card(Modifier::new().fill_max_width(), || {
        column_with(Modifier::new().fill_max_width().padding(20.0), || {
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpaceBetween,
                Alignment::Start,
                || {
                    column(|| {
                        title(metric.label, 16.0);
                        muted_sized(
                            [
                                "Total for the last 3 months",
                                "Total for the last 30 days",
                                "Total for the last 7 days",
                            ][range.get().min(2)],
                            13.0,
                        );
                    });
                    let pick = range.clone();
                    segmented_control(
                        range.get(),
                        &["Last 3 months", "Last 30 days", "Last 7 days"],
                        move |index| pick.set(index),
                    );
                },
            );
            spacer(Modifier::new().height(16.0));
            stack_aligned(
                Modifier::new().fill_max_width().height(300.0).chain(hover),
                BoxAlignment::TopStart,
                || {
                    area_chart(
                        Modifier::new().fill_max_size(),
                        vec![
                            Series::new(second, theme.primary).filled(),
                            Series::new(first, theme.on_surface_muted).filled(),
                        ],
                        labels,
                        if points > 30 { 8 } else { 3 },
                        marker,
                        chart_style(),
                    );
                    // The readout sits beside the marker, on the left near the right edge.
                    if let (Some(marker), Some(readout)) = (marker, &readout) {
                        const TIP: f32 = 144.0;
                        let x = width.get() * marker as f32 / (points - 1).max(1) as f32;
                        let x = if x + TIP + 24.0 > width.get() {
                            x - TIP - 12.0
                        } else {
                            x + 12.0
                        };
                        stack_with(
                            Modifier::new()
                                .offset(x.max(0.0), 30.0)
                                .shadow(8.0, 8.0)
                                .background_rounded(theme.surface, 8.0)
                                .border(1.0, theme.outline, 8.0)
                                .padding_xy(12.0, 8.0),
                            || {
                                column(|| {
                                    title(&readout.0, 12.0);
                                    spacer(Modifier::new().height(4.0));
                                    for (name, value) in
                                        [(metric.names.0, readout.1), (metric.names.1, readout.2)]
                                    {
                                        key(name, || {
                                            row_arranged(
                                                Modifier::new().width(120.0),
                                                Arrangement::SpaceBetween,
                                                Alignment::Center,
                                                || {
                                                    muted_sized(name, 12.0);
                                                    title(&value.to_string(), 12.0);
                                                },
                                            );
                                        });
                                    }
                                });
                            },
                        );
                    }
                },
            );
        });
    });
}

// ------------------------------------------------------------------ table

const COLUMNS: [(&str, f32); 5] = [
    ("Section Type", 2.0),
    ("Status", 1.6),
    ("Target", 1.0),
    ("Limit", 1.0),
    ("Reviewer", 1.8),
];
const TABS: [&str; 4] = [
    "Outline",
    "Past Performance",
    "Key Personnel",
    "Focus Documents",
];

fn matches_tab(doc: &Doc, tab: usize) -> bool {
    match tab {
        1 => doc.done,
        2 => !doc.done,
        3 => doc.kind == "Narrative",
        _ => true,
    }
}

fn compare(a: &Doc, b: &Doc, column: usize) -> std::cmp::Ordering {
    match column {
        0 => a.header.to_lowercase().cmp(&b.header.to_lowercase()),
        1 => a.kind.cmp(&b.kind),
        2 => a.done.cmp(&b.done),
        3 => a.target.cmp(&b.target),
        4 => a.limit.cmp(&b.limit),
        _ => a.reviewer.cmp(&b.reviewer),
    }
}

#[composable]
fn document_section(ctx: &Ctx) {
    let tab = remember(|| 0usize);
    let query = remember_text_field_state("");
    let visible = remember(|| [true; 5]);
    let sort = remember(|| None::<(usize, bool)>);
    let selected = remember(Vec::<usize>::new);
    let page_index = remember(|| 0usize);
    let page_size = remember(|| 5usize);

    let all = ctx.docs.to_vec();
    let counts = [
        all.len(),
        all.iter().filter(|d| d.done).count(),
        all.iter().filter(|d| !d.done).count(),
        all.iter().filter(|d| d.kind == "Narrative").count(),
    ];
    let needle = query.text().to_lowercase();
    let mut rows: Vec<Doc> = all
        .into_iter()
        .filter(|doc| matches_tab(doc, tab.get()) && doc.header.to_lowercase().contains(&needle))
        .collect();
    if let Some((column, ascending)) = sort.get() {
        rows.sort_by(|a, b| {
            let order = compare(a, b, column);
            if ascending { order } else { order.reverse() }
        });
    }
    let pages = rows.len().div_ceil(page_size.get()).max(1);
    let current = page_index.get().min(pages - 1);
    let shown: Vec<Doc> = rows
        .iter()
        .skip(current * page_size.get())
        .take(page_size.get())
        .cloned()
        .collect();
    let chosen: Vec<usize> = selected
        .get()
        .into_iter()
        .filter(|id| rows.iter().any(|doc| doc.id == *id))
        .collect();

    // Toolbar.
    row_arranged(
        Modifier::new().fill_max_width(),
        Arrangement::SpaceBetween,
        Alignment::Center,
        || {
            let pick = tab.clone();
            let page = page_index.clone();
            tabs_list(
                tab.get(),
                &[
                    (TABS[0], None),
                    (TABS[1], Some(counts[1] as u32)),
                    (TABS[2], Some(counts[2] as u32)),
                    (TABS[3], None),
                ],
                move |index| {
                    pick.set(index);
                    page.set(0);
                },
            );
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(8.0),
                Alignment::Center,
                || {
                    if !chosen.is_empty() {
                        let (ctx, ids, selected) = (ctx.clone(), chosen.clone(), selected.clone());
                        styled_button(
                            Modifier::new(),
                            ButtonStyle::Destructive,
                            &format!("Delete {}", chosen.len()),
                            None,
                            None,
                            move || {
                                ctx.delete_docs(&ids);
                                selected.set(Vec::new());
                                ctx.say(format!("Deleted {} sections", ids.len()));
                            },
                        );
                    }
                    stack_with(Modifier::new().width(190.0), || {
                        let page = page_index.clone();
                        query.on_submit(move || page.set(0));
                        text_field(
                            Modifier::new().fill_max_width(),
                            &query,
                            "Filter headers...",
                        );
                    });
                    let items = COLUMNS
                        .iter()
                        .enumerate()
                        .map(|(index, (name, _))| {
                            let toggle = visible.clone();
                            MenuItem::new(*name, move || {
                                toggle.update(|v| v[index] = !v[index]);
                            })
                            .checked(visible.get()[index])
                        })
                        .collect();
                    menu(
                        |open| {
                            styled_button(
                                Modifier::new(),
                                ButtonStyle::Outline,
                                "Customize Columns",
                                Some(Icon::Layers),
                                Some(Icon::ChevronDown),
                                move || open(),
                            )
                        },
                        items,
                    );
                    let add = ctx.clone();
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Outline,
                        "Add Section",
                        Some(Icon::Plus),
                        None,
                        move || add.open(Overlay::NewDocument),
                    );
                },
            );
        },
    );
    spacer(Modifier::new().height(12.0));

    let theme = theme();
    card(Modifier::new().fill_max_width(), || {
        column_with(Modifier::new().fill_max_width(), || {
            // Header.
            stack_with(
                Modifier::new().fill_max_width().background(theme.secondary),
                || {
                    row_arranged(
                        Modifier::new().fill_max_width().padding_xy(12.0, 10.0),
                        Arrangement::Start,
                        Alignment::Center,
                        || {
                            table_cell(0.6, || {
                                let all_chosen = !shown.is_empty()
                                    && shown.iter().all(|doc| chosen.contains(&doc.id));
                                let (selected, ids) = (
                                    selected.clone(),
                                    shown.iter().map(|d| d.id).collect::<Vec<_>>(),
                                );
                                checkbox(all_chosen, move |value| {
                                    selected.set(if value { ids.clone() } else { Vec::new() })
                                });
                            });
                            sort_header(3.0, "Header", 0, &sort);
                            for (index, (name, weight)) in COLUMNS.iter().enumerate() {
                                if visible.get()[index] {
                                    sort_header(*weight, name, index + 1, &sort);
                                }
                            }
                            table_cell(0.4, || {});
                        },
                    );
                },
            );
            if shown.is_empty() {
                stack_aligned(
                    Modifier::new().fill_max_width().padding(32.0),
                    BoxAlignment::Center,
                    || muted("No results."),
                );
            }
            for doc in &shown {
                key(doc.id, || {
                    document_row(ctx, doc, &visible, &selected, chosen.contains(&doc.id));
                });
            }
            // Footer.
            row_arranged(
                Modifier::new().fill_max_width().padding_xy(14.0, 10.0),
                Arrangement::SpaceBetween,
                Alignment::Center,
                || {
                    muted_sized(
                        &format!("{} of {} row(s) selected.", chosen.len(), rows.len()),
                        13.0,
                    );
                    row_arranged(
                        Modifier::new(),
                        Arrangement::SpacedBy(12.0),
                        Alignment::Center,
                        || {
                            muted_sized("Rows per page", 13.0);
                            let size = page_size.clone();
                            let reset = page_index.clone();
                            select(
                                Modifier::new().width(76.0),
                                ["5", "10", "20"].map(String::from).to_vec(),
                                [5, 10, 20]
                                    .iter()
                                    .position(|n| *n == page_size.get())
                                    .unwrap_or(0),
                                move |index| {
                                    size.set([5, 10, 20][index]);
                                    reset.set(0);
                                },
                            );
                            muted_sized(&format!("Page {} of {}", current + 1, pages), 13.0);
                            for (glyph, label, target) in [
                                (
                                    Icon::ChevronLeft,
                                    "Previous page",
                                    current.saturating_sub(1),
                                ),
                                (
                                    Icon::ChevronRight,
                                    "Next page",
                                    (current + 1).min(pages - 1),
                                ),
                            ] {
                                key(label, || {
                                    let go = page_index.clone();
                                    icon_box(glyph, 30.0, label, move || go.set(target));
                                });
                            }
                        },
                    );
                },
            );
        });
    });
}

#[composable]
fn sort_header(weight: f32, label: &str, column: usize, sort: &State<Option<(usize, bool)>>) {
    let theme = theme();
    let state = sort.get();
    let glyph = match state {
        Some((c, true)) if c == column => Icon::ChevronUp,
        Some((c, false)) if c == column => Icon::ChevronDown,
        _ => Icon::ChevronsUpDown,
    };
    let sort = sort.clone();
    table_cell(weight, || {
        row_arranged(
            Modifier::new()
                .semantics(
                    Semantics::new(Role::Button)
                        .label(format!("Sort by {label}"))
                        .merging_descendants(),
                )
                .clickable(move || {
                    sort.set(match sort.get() {
                        Some((c, true)) if c == column => Some((column, false)),
                        Some((c, false)) if c == column => None,
                        _ => Some((column, true)),
                    })
                }),
            Arrangement::SpacedBy(4.0),
            Alignment::Center,
            || {
                muted(label);
                icon(glyph, 12.0, theme.on_surface_muted);
            },
        );
    });
}

#[composable]
fn document_row(
    ctx: &Ctx,
    doc: &Doc,
    visible: &State<[bool; 5]>,
    selected: &State<Vec<usize>>,
    chosen: bool,
) {
    let theme = theme();
    let id = doc.id;
    let shown = visible.get();
    table_row(|| {
        table_cell(0.6, || {
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(10.0),
                Alignment::Center,
                || {
                    icon(Icon::Grip, 14.0, theme.on_surface_muted);
                    let selected = selected.clone();
                    checkbox(chosen, move |value| {
                        selected.update(|ids| {
                            ids.retain(|i| *i != id);
                            if value {
                                ids.push(id);
                            }
                        })
                    });
                },
            );
        });
        table_cell(3.0, || {
            let open = ctx.clone();
            stack_with(
                Modifier::new()
                    .semantics(Semantics::new(Role::Button).label(doc.header.clone()))
                    .clickable(move || open.open(Overlay::Detail(id))),
                || title(&doc.header, 14.0),
            );
        });
        if shown[0] {
            table_cell(2.0, || pill(&doc.kind, BadgeStyle::Outline));
        }
        if shown[1] {
            table_cell(1.6, || {
                let toggle = ctx.clone();
                row_arranged(
                    Modifier::new()
                        .semantics(
                            Semantics::new(Role::Button)
                                .label("Toggle status".to_string())
                                .merging_descendants(),
                        )
                        .clickable(move || toggle.change_doc(id, |d| d.done = !d.done)),
                    Arrangement::SpacedBy(6.0),
                    Alignment::Center,
                    || {
                        spacer(Modifier::new().size(8.0, 8.0).background_rounded(
                            if doc.done {
                                theme.success
                            } else {
                                theme.on_surface_muted
                            },
                            4.0,
                        ));
                        text(if doc.done { "Done" } else { "In Process" });
                    },
                );
            });
        }
        if shown[2] {
            table_cell(1.0, || text(doc.target.to_string()));
        }
        if shown[3] {
            table_cell(1.0, || text(doc.limit.to_string()));
        }
        if shown[4] {
            table_cell(1.8, || text(doc.reviewer.clone()));
        }
        table_cell(0.4, || {
            let items = vec![
                MenuItem::new("Open details", {
                    let ctx = ctx.clone();
                    move || ctx.open(Overlay::Detail(id))
                })
                .icon(Icon::File),
                MenuItem::new(
                    if doc.done {
                        "Mark in process"
                    } else {
                        "Mark done"
                    },
                    {
                        let ctx = ctx.clone();
                        move || ctx.change_doc(id, |d| d.done = !d.done)
                    },
                )
                .icon(Icon::Check),
                MenuItem::new("Duplicate", {
                    let (ctx, doc) = (ctx.clone(), doc.clone());
                    move || {
                        ctx.add_doc(&format!("{} (copy)", doc.header), &doc.kind);
                        ctx.say("Section duplicated");
                    }
                })
                .icon(Icon::Layers),
                MenuItem::new("Delete", {
                    let ctx = ctx.clone();
                    move || {
                        ctx.delete_docs(&[id]);
                        ctx.say("Section deleted");
                    }
                })
                .icon(Icon::X)
                .destructive(),
            ];
            menu(
                |open| icon_action(Icon::MoreVertical, "Row actions", move || open()),
                items,
            );
        });
    });
}
