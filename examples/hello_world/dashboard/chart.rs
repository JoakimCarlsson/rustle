//! The visitors chart.

use rustle::prelude::*;

use super::Metric;
use crate::ui::{chart_style, wander};

pub(super) fn day_label(offset: usize) -> String {
    if offset < 30 {
        format!("Apr {}", offset + 1)
    } else if offset < 61 {
        format!("May {}", offset - 29)
    } else {
        format!("Jun {}", offset - 60)
    }
}

#[composable]
pub(super) fn visitors_chart(metric: &Metric) {
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
