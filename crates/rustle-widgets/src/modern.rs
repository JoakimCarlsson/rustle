//! Components in a restrained, monochrome style: hairline borders, pill badges,
//! icon buttons, segmented controls, tabs and a month calendar.

use std::cell::Cell;
use std::rc::Rc;

use rustle_foundation::{
    Alignment, Arrangement, BoxAlignment, LOCAL_TEXT_STYLE, Modifier, Role, Semantics, column,
    row_arranged, spacer, stack_aligned, stack_with, text, text_styled,
};
use rustle_geometry::{Rect, Size};
use rustle_graphics::Color;
use rustle_runtime::key;
use rustle_text::FontWeight;

use crate::icons::{Icon, icon};
use crate::theme;

/// How a button looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// A solid fill in the foreground color.
    Default,
    /// A subtle fill.
    Secondary,
    /// A border and no fill.
    Outline,
    /// Nothing until it is hovered.
    Ghost,
    /// A warning tint.
    Destructive,
}

/// Emits a button showing `label`, with an optional icon before and after it.
#[track_caller]
pub fn styled_button(
    modifier: Modifier,
    style: ButtonStyle,
    label: &str,
    leading: Option<Icon>,
    trailing: Option<Icon>,
    on_click: impl Fn() + 'static,
) {
    let theme = theme();
    let (fill, content) = match style {
        ButtonStyle::Default => (theme.primary, theme.on_primary),
        ButtonStyle::Secondary => (theme.accent, theme.on_surface),
        ButtonStyle::Outline | ButtonStyle::Ghost => (Color::TRANSPARENT, theme.on_surface),
        ButtonStyle::Destructive => (
            Color::rgba(theme.danger.r, theme.danger.g, theme.danger.b, 36),
            theme.danger,
        ),
    };
    let border = if style == ButtonStyle::Outline {
        theme.outline
    } else {
        Color::TRANSPARENT
    };
    let padding = if leading.is_some() || trailing.is_some() {
        12.0
    } else {
        16.0
    };
    let label_text = label.to_string();
    row_arranged(
        modifier
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.to_string())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .background_rounded(fill, theme.corner_radius)
            .border(1.0, border, theme.corner_radius)
            .padding_xy(padding, 7.0),
        Arrangement::Center,
        Alignment::Center,
        || {
            if let Some(leading) = leading {
                icon(leading, 16.0, content);
                spacer(Modifier::new().width(8.0));
            }
            let text_style = LOCAL_TEXT_STYLE
                .current()
                .color(content)
                .font_weight(FontWeight::MEDIUM);
            LOCAL_TEXT_STYLE.provide(text_style, || text(label_text));
            if let Some(trailing) = trailing {
                spacer(Modifier::new().width(8.0));
                icon(trailing, 16.0, content);
            }
        },
    );
}

/// How a badge looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeStyle {
    Default,
    Secondary,
    Outline,
}

/// Emits a small rounded label such as a status or a count.
#[track_caller]
pub fn pill(label: &str, style: BadgeStyle) {
    let theme = theme();
    let (fill, content, border) = match style {
        BadgeStyle::Default => (theme.primary, theme.on_primary, Color::TRANSPARENT),
        BadgeStyle::Secondary => (theme.accent, theme.on_surface, Color::TRANSPARENT),
        BadgeStyle::Outline => (Color::TRANSPARENT, theme.on_surface, theme.outline),
    };
    stack_with(
        Modifier::new()
            .background_rounded(fill, 100.0)
            .border(1.0, border, 100.0)
            .padding_xy(8.0, 1.0),
        || {
            let text_style = LOCAL_TEXT_STYLE
                .current()
                .color(content)
                .font_size(12.0)
                .font_weight(FontWeight::MEDIUM);
            text_styled(Modifier::new(), label.to_string(), text_style);
        },
    );
}

/// Emits a round avatar of `size` showing `initials`.
#[track_caller]
pub fn avatar(initials: &str, size: f32) {
    let theme = theme();
    stack_aligned(
        Modifier::new()
            .size(size, size)
            .background_rounded(theme.accent, size / 2.0),
        BoxAlignment::Center,
        || {
            let text_style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.on_surface)
                .font_size((size * 0.38).max(10.0))
                .font_weight(FontWeight::MEDIUM);
            text_styled(Modifier::new(), initials.to_string(), text_style);
        },
    );
}

/// Emits a hairline across the available width.
#[track_caller]
pub fn separator() {
    let theme = theme();
    spacer(
        Modifier::new()
            .fill_max_width()
            .height(1.0)
            .background(theme.outline),
    );
}

/// Emits text in the muted color, for descriptions and captions.
#[track_caller]
pub fn muted(content: &str) {
    muted_sized(content, 14.0);
}

/// Emits muted text of a given size.
#[track_caller]
pub fn muted_sized(content: &str, size: f32) {
    let theme = theme();
    let text_style = LOCAL_TEXT_STYLE
        .current()
        .color(theme.on_surface_muted)
        .font_size(size);
    text_styled(Modifier::new(), content.to_string(), text_style);
}

/// Emits text in a semibold weight, for titles.
#[track_caller]
pub fn title(content: &str, size: f32) {
    let text_style = LOCAL_TEXT_STYLE
        .current()
        .font_size(size)
        .font_weight(FontWeight::SEMI_BOLD);
    text_styled(Modifier::new(), content.to_string(), text_style);
}

/// Emits a card heading: a title with a muted line of description under it.
#[track_caller]
pub fn card_header(heading: &str, description: &str) {
    column(|| {
        title(heading, 14.0);
        if !description.is_empty() {
            muted_sized(description, 13.0);
        }
    });
}

/// Emits a bordered box showing `value` with a chevron, which calls `on_click` when clicked.
#[track_caller]
pub fn select_field(modifier: Modifier, value: &str, on_click: impl Fn() + 'static) {
    let theme = theme();
    row_arranged(
        modifier
            .semantics(
                Semantics::new(Role::Button)
                    .label(value.to_string())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .background_rounded(theme.secondary, theme.corner_radius)
            .border(1.0, theme.outline, theme.corner_radius)
            .padding_xy(12.0, 8.0),
        Arrangement::SpaceBetween,
        Alignment::Center,
        || {
            text(value.to_string());
            icon(Icon::ChevronsUpDown, 14.0, theme.on_surface_muted);
        },
    );
}

/// Emits joined buttons, one per label, with `selected` highlighted.
#[track_caller]
pub fn segmented_control(selected: usize, labels: &[&str], on_select: impl Fn(usize) + 'static) {
    let theme = theme();
    let on_select = Rc::new(on_select);
    row_arranged(
        Modifier::new()
            .border(1.0, theme.outline, theme.corner_radius)
            .clip(),
        Arrangement::Start,
        Alignment::Center,
        || {
            for (index, label) in labels.iter().enumerate() {
                let on_select = Rc::clone(&on_select);
                key(index, || {
                    let active = index == selected;
                    stack_with(
                        Modifier::new()
                            .semantics(
                                Semantics::new(Role::Tab)
                                    .label(label.to_string())
                                    .toggled(active)
                                    .merging_descendants(),
                            )
                            .clickable(move || on_select(index))
                            .background(if active {
                                theme.accent
                            } else {
                                Color::TRANSPARENT
                            })
                            .padding_xy(14.0, 6.0),
                        || {
                            let text_style = LOCAL_TEXT_STYLE
                                .current()
                                .color(if active {
                                    theme.on_surface
                                } else {
                                    theme.on_surface_muted
                                })
                                .font_weight(FontWeight::MEDIUM);
                            text_styled(Modifier::new(), label.to_string(), text_style);
                        },
                    );
                });
            }
        },
    );
}

/// Emits tabs in a rounded tray, one per `(label, count)`, with `selected` raised.
#[track_caller]
pub fn tabs_list(
    selected: usize,
    tabs: &[(&str, Option<u32>)],
    on_select: impl Fn(usize) + 'static,
) {
    let theme = theme();
    let on_select = Rc::new(on_select);
    row_arranged(
        Modifier::new()
            .background_rounded(theme.secondary, theme.corner_radius + 2.0)
            .border(1.0, theme.outline, theme.corner_radius + 2.0)
            .padding(3.0),
        Arrangement::SpacedBy(2.0),
        Alignment::Center,
        || {
            for (index, (label, count)) in tabs.iter().enumerate() {
                let on_select = Rc::clone(&on_select);
                key(index, || {
                    let active = index == selected;
                    row_arranged(
                        Modifier::new()
                            .semantics(
                                Semantics::new(Role::Tab)
                                    .label(label.to_string())
                                    .toggled(active)
                                    .merging_descendants(),
                            )
                            .clickable(move || on_select(index))
                            .background_rounded(
                                if active {
                                    theme.accent
                                } else {
                                    Color::TRANSPARENT
                                },
                                theme.corner_radius,
                            )
                            .padding_xy(12.0, 5.0),
                        Arrangement::SpacedBy(6.0),
                        Alignment::Center,
                        || {
                            let text_style = LOCAL_TEXT_STYLE
                                .current()
                                .color(if active {
                                    theme.on_surface
                                } else {
                                    theme.on_surface_muted
                                })
                                .font_weight(FontWeight::MEDIUM);
                            text_styled(Modifier::new(), label.to_string(), text_style);
                            if let Some(count) = count {
                                pill(&count.to_string(), BadgeStyle::Secondary);
                            }
                        },
                    );
                });
            }
        },
    );
}

/// Emits a table row: its cells, with a hairline under it.
#[track_caller]
pub fn table_row(cells: impl FnOnce()) {
    let theme = theme();
    column(|| {
        row_arranged(
            Modifier::new().fill_max_width().padding_xy(12.0, 10.0),
            Arrangement::Start,
            Alignment::Center,
            cells,
        );
        spacer(
            Modifier::new()
                .fill_max_width()
                .height(1.0)
                .background(theme.outline),
        );
    });
}

/// Emits one cell of a table row, sharing the row's width in proportion to `weight`.
#[track_caller]
pub fn table_cell(weight: f32, content: impl FnOnce()) {
    stack_with(Modifier::new().weight(weight), content);
}

/// Emits a month as a grid of days. `first_weekday` is the weekday of the first
/// day, 0 for Sunday. Days from `range.0` to `range.1` are marked, the ends
/// solid. `on_select` gets the day clicked.
#[track_caller]
pub fn calendar(
    heading: &str,
    first_weekday: usize,
    days: usize,
    range: Option<(usize, usize)>,
    on_navigate: impl Fn(i32) + 'static,
    on_select: impl Fn(usize) + 'static,
) {
    const CELL: f32 = 34.0;
    let theme = theme();
    let on_select = Rc::new(on_select);
    let on_navigate = Rc::new(on_navigate);
    column(|| {
        row_arranged(
            Modifier::new().width(CELL * 7.0),
            Arrangement::SpaceBetween,
            Alignment::Center,
            || {
                let (back, forward) = (Rc::clone(&on_navigate), Rc::clone(&on_navigate));
                stack_aligned(
                    Modifier::new()
                        .size(28.0, 28.0)
                        .semantics(Semantics::new(Role::Button).label("Previous month".to_string()))
                        .clickable(move || back(-1)),
                    BoxAlignment::Center,
                    || icon(Icon::ChevronLeft, 16.0, theme.on_surface_muted),
                );
                title(heading, 14.0);
                stack_aligned(
                    Modifier::new()
                        .size(28.0, 28.0)
                        .semantics(Semantics::new(Role::Button).label("Next month".to_string()))
                        .clickable(move || forward(1)),
                    BoxAlignment::Center,
                    || icon(Icon::ChevronRight, 16.0, theme.on_surface_muted),
                );
            },
        );
        spacer(Modifier::new().height(10.0));
        row_arranged(
            Modifier::new(),
            Arrangement::Start,
            Alignment::Center,
            || {
                for label in ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"] {
                    stack_aligned(
                        Modifier::new().size(CELL, 24.0),
                        BoxAlignment::Center,
                        || muted_sized(label, 12.0),
                    );
                }
            },
        );
        let rows = (first_weekday + days).div_ceil(7);
        for week in 0..rows {
            key(week, || {
                row_arranged(
                    Modifier::new(),
                    Arrangement::Start,
                    Alignment::Center,
                    || {
                        for weekday in 0..7 {
                            let slot = week * 7 + weekday;
                            let day = (slot + 1)
                                .checked_sub(first_weekday)
                                .filter(|d| *d >= 1 && *d <= days);
                            key(weekday, || {
                                let on_select = Rc::clone(&on_select);
                                let Some(day) = day else {
                                    spacer(Modifier::new().size(CELL, CELL));
                                    return;
                                };
                                let (end, inside) = match range {
                                    Some((from, to)) => {
                                        (day == from || day == to, day > from && day < to)
                                    }
                                    None => (false, false),
                                };
                                let fill = if end {
                                    theme.primary
                                } else if inside {
                                    theme.accent
                                } else {
                                    Color::TRANSPARENT
                                };
                                let content = if end {
                                    theme.on_primary
                                } else {
                                    theme.on_surface
                                };
                                stack_aligned(
                                    Modifier::new()
                                        .size(CELL, CELL)
                                        .padding(2.0)
                                        .semantics(
                                            Semantics::new(Role::Button).label(day.to_string()),
                                        )
                                        .clickable(move || on_select(day))
                                        .background_rounded(fill, theme.corner_radius),
                                    BoxAlignment::Center,
                                    || {
                                        let text_style = LOCAL_TEXT_STYLE.current().color(content);
                                        text_styled(Modifier::new(), day.to_string(), text_style);
                                    },
                                );
                            });
                        }
                    },
                );
            });
        }
    });
}

/// The weekday of the first of `month` (1 to 12) in `year`, 0 for Sunday.
pub fn first_weekday(year: i32, month: u32) -> usize {
    // Sakamoto's method.
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let year = if month < 3 { year - 1 } else { year };
    ((year + year / 4 - year / 100 + year / 400 + OFFSETS[(month - 1) as usize] + 1) % 7) as usize
}

/// How many days `month` (1 to 12) has in `year`.
pub fn days_in_month(year: i32, month: u32) -> usize {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        _ => 28,
    }
}

/// One entry of a menu.
#[derive(Clone)]
pub struct MenuItem {
    label: String,
    icon: Option<Icon>,
    on_select: Rc<dyn Fn()>,
    destructive: bool,
    checked: bool,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, on_select: impl Fn() + 'static) -> Self {
        Self {
            label: label.into(),
            icon: None,
            on_select: Rc::new(on_select),
            destructive: false,
            checked: false,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Draws the entry as a warning, for deleting things.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// Marks the entry with a check, for the current choice.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

/// Emits `trigger`, which is given a function that opens a menu of `items`
/// below it. Choosing an item closes the menu and runs the item.
#[track_caller]
pub fn menu(trigger: impl FnOnce(Rc<dyn Fn()>), items: Vec<MenuItem>) {
    rustle_runtime::group(|| {
        let theme = theme();
        let open = rustle_runtime::remember(|| false);
        let opener = open.clone();
        let dismiss = open.clone();
        // Where the trigger was last drawn, so the menu can stay inside the window.
        let placed = rustle_runtime::remember(|| Rc::new(Cell::new(None::<(Rect, Size)>))).get();
        let report = Rc::clone(&placed);
        let width = placed
            .get()
            .map_or(200.0, |(rect, _)| rect.size.width.clamp(200.0, 360.0));
        let height = items.len() as f32 * 36.0 + 16.0;
        let offset = placed.get().map_or((0.0, 4.0), |(rect, window)| {
            let across = if rect.origin.x + width > window.width - 8.0 {
                rect.size.width - width
            } else {
                0.0
            };
            let down = if rect.origin.y + rect.size.height + height + 8.0 > window.height {
                -(rect.size.height + height + 4.0)
            } else {
                4.0
            };
            (across, down)
        });
        column(|| {
            stack_with(
                Modifier::new().on_placed(move |rect, window| report.set(Some((rect, window)))),
                || trigger(Rc::new(move || opener.set(true))),
            );
            crate::dropdown_menu_at(
                open.get(),
                move || dismiss.set(false),
                offset,
                Some(width),
                || {
                    for (index, item) in items.iter().enumerate() {
                        let (item, close) = (item.clone(), open.clone());
                        key(index, || {
                            let color = if item.destructive {
                                theme.danger
                            } else {
                                theme.on_surface
                            };
                            row_arranged(
                                Modifier::new()
                                    .fill_max_width()
                                    .semantics(
                                        Semantics::new(Role::Button)
                                            .label(item.label.clone())
                                            .merging_descendants(),
                                    )
                                    .clickable({
                                        let (handler, close) =
                                            (Rc::clone(&item.on_select), close.clone());
                                        move || {
                                            close.set(false);
                                            handler();
                                        }
                                    })
                                    .padding_xy(14.0, 8.0),
                                Arrangement::SpacedBy(10.0),
                                Alignment::Center,
                                || {
                                    if let Some(glyph) = item.icon {
                                        icon(glyph, 15.0, color);
                                    }
                                    let style = LOCAL_TEXT_STYLE.current().color(color);
                                    text_styled(Modifier::new(), item.label.clone(), style);
                                    if item.checked {
                                        spacer(Modifier::new().weight(1.0));
                                        icon(Icon::Check, 14.0, theme.on_surface_muted);
                                    }
                                },
                            );
                        });
                    }
                },
            );
        });
    });
}

/// Emits a box showing the `selected` one of `options`; clicking it opens a menu of all of them,
/// and choosing one calls `on_change` with its index.
#[track_caller]
pub fn select(
    modifier: Modifier,
    options: Vec<String>,
    selected: usize,
    on_change: impl Fn(usize) + 'static,
) {
    let on_change = Rc::new(on_change);
    let value = options.get(selected).cloned().unwrap_or_default();
    let items = options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            let on_change = Rc::clone(&on_change);
            MenuItem::new(option.clone(), move || on_change(index)).checked(index == selected)
        })
        .collect();
    menu(|open| select_field(modifier, &value, move || open()), items);
}
