//! Tables and the month calendar.

use std::rc::Rc;

use rustle_foundation::{
    Alignment, Arrangement, BoxAlignment, LOCAL_TEXT_STYLE, Modifier, Role, Semantics, column,
    row_arranged, spacer, stack_aligned, stack_with, text_styled,
};
use rustle_graphics::Color;
use rustle_icons::phosphor;
use rustle_runtime::key;

use crate::icons::icon;
use crate::muted_sized;
use crate::shape::ShapeModifier;
use crate::theme::{color_scheme, shapes};
use crate::title;

/// Emits a table row: its cells, with a hairline under it.
#[track_caller]
pub fn table_row(cells: impl FnOnce()) {
    let theme = color_scheme();
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
    let theme = color_scheme();
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
                    || icon(phosphor::CARET_LEFT, 16.0, theme.on_surface_variant),
                );
                title(heading, 14.0);
                stack_aligned(
                    Modifier::new()
                        .size(28.0, 28.0)
                        .semantics(Semantics::new(Role::Button).label("Next month".to_string()))
                        .clickable(move || forward(1)),
                    BoxAlignment::Center,
                    || icon(phosphor::CARET_RIGHT, 16.0, theme.on_surface_variant),
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
                                    theme.surface_variant
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
                                        .background_shape(fill, shapes().small),
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
