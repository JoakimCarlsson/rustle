use rustle_foundation::{Modifier, canvas};
use rustle_graphics::{Color, TextAlign};

use crate::axes::*;
use crate::style::*;

/// Emits an area chart: each series is a smooth line, and filled series fade
/// toward the bottom. Size it with `modifier`; `labels` run along the bottom,
/// every `label_every`th one shown.
#[track_caller]
pub fn area_chart(
    modifier: Modifier,
    series: Vec<Series>,
    labels: Vec<String>,
    label_every: usize,
    marker: Option<usize>,
    style: ChartStyle,
) {
    canvas(modifier, move |canvas| {
        let (width, height) = (canvas.width(), canvas.height());
        let space = if labels.iter().all(String::is_empty) {
            12.0
        } else {
            34.0
        };
        let plot = (0.0, 6.0, width, (height - space).max(10.0));
        grid_and_labels(canvas, style, plot, &labels, label_every, false);
        let (minimum, maximum) = value_range(&series, style);
        for series in &series {
            let count = series.values.len();
            if count < 2 {
                continue;
            }
            let points: Vec<(f32, f32)> = series
                .values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    (
                        plot.0 + plot.2 * index as f32 / (count - 1) as f32,
                        plot.1 + plot.3 * (1.0 - (value - minimum) / (maximum - minimum)),
                    )
                })
                .collect();
            let curve = smooth(&points, 3.0);
            if series.fill {
                let top = Color {
                    a: (f32::from(series.color.a) * 0.4) as u8,
                    ..series.color
                };
                let bottom = Color {
                    a: 0,
                    ..series.color
                };
                canvas.fill_below(&curve, plot.1 + plot.3, top, bottom);
            }
            canvas.polyline(&curve, series.width, series.color);
            if let Some(index) = marker.filter(|index| *index < count) {
                canvas.circle(points[index].0, points[index].1, 4.0, style.surface);
                canvas.stroke_circle(points[index].0, points[index].1, 4.0, 1.5, series.color);
            }
        }
        if let Some(index) = marker
            && let Some(longest) = series.iter().map(|s| s.values.len()).max()
            && longest > 1
        {
            let x = plot.0 + plot.2 * index as f32 / (longest - 1) as f32;
            canvas.line((x, plot.1), (x, plot.1 + plot.3), 1.0, style.label);
        }
    });
}

/// Emits a line chart: each series is a smooth line, optionally with a dot at each value.
#[track_caller]
pub fn line_chart(
    modifier: Modifier,
    series: Vec<Series>,
    labels: Vec<String>,
    label_every: usize,
    markers: bool,
    style: ChartStyle,
) {
    canvas(modifier, move |canvas| {
        let (width, height) = (canvas.width(), canvas.height());
        let inset = 8.0;
        let space = if labels.iter().all(String::is_empty) {
            12.0
        } else {
            34.0
        };
        let plot = (inset, 6.0, width - inset * 2.0, (height - space).max(10.0));
        grid_and_labels(canvas, style, plot, &labels, label_every, false);
        let (minimum, maximum) = value_range(&series, style);
        for series in &series {
            let count = series.values.len();
            if count < 2 {
                continue;
            }
            let points: Vec<(f32, f32)> = series
                .values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    (
                        plot.0 + plot.2 * index as f32 / (count - 1) as f32,
                        plot.1 + plot.3 * (1.0 - (value - minimum) / (maximum - minimum)),
                    )
                })
                .collect();
            canvas.polyline(&smooth(&points, 3.0), series.width, series.color);
            if markers {
                for (x, y) in &points {
                    canvas.circle(*x, *y, 3.5, series.color);
                    canvas.circle(*x, *y, 1.8, style.surface);
                }
            }
        }
    });
}

/// Emits a bar chart with a label under each bar. Bars blend from `top` to
/// `bottom`; `highlight` picks out one bar, drawn solid in `highlight_color`.
#[track_caller]
#[allow(clippy::too_many_arguments)]
pub fn bar_chart(
    modifier: Modifier,
    values: Vec<f32>,
    labels: Vec<String>,
    top: Color,
    bottom: Color,
    highlight: Option<(usize, Color)>,
    show_labels: bool,
    style: ChartStyle,
) {
    canvas(modifier, move |canvas| {
        let (width, height) = (canvas.width(), canvas.height());
        let label_space = if show_labels { 26.0 } else { 0.0 };
        let plot_height = (height - label_space).max(10.0);
        let maximum = values.iter().copied().fold(0.0_f32, f32::max).max(0.0001);
        let slot = width / values.len().max(1) as f32;
        let bar_width = slot * 0.62;
        for (index, value) in values.iter().enumerate() {
            let bar_height = (plot_height * value / maximum).max(2.0);
            let x = slot * index as f32 + (slot - bar_width) / 2.0;
            let y = plot_height - bar_height;
            match highlight {
                Some((chosen, color)) if chosen == index => {
                    canvas.rounded_rect(x, y, bar_width, bar_height, 6.0, color);
                }
                _ => canvas.gradient(x, y, bar_width, bar_height, 6.0, top, bottom, 90.0),
            }
            if show_labels && let Some(label) = labels.get(index) {
                canvas.text(
                    slot * index as f32,
                    plot_height + 8.0,
                    slot,
                    label,
                    style.label_size,
                    400,
                    style.label,
                    TextAlign::Center,
                );
            }
        }
    });
}

/// Emits a tiny line chart with no axes.
#[track_caller]
pub fn sparkline(modifier: Modifier, values: Vec<f32>, color: Color, width: f32) {
    canvas(modifier, move |canvas| {
        let (w, h) = (canvas.width(), canvas.height());
        let count = values.len();
        if count < 2 {
            return;
        }
        let low = values.iter().copied().fold(f32::MAX, f32::min);
        let high = values.iter().copied().fold(f32::MIN, f32::max);
        let span = (high - low).max(0.0001);
        let inset = width;
        let points: Vec<(f32, f32)> = values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                (
                    inset + (w - inset * 2.0) * index as f32 / (count - 1) as f32,
                    inset + (h - inset * 2.0) * (1.0 - (value - low) / span),
                )
            })
            .collect();
        canvas.polyline(&smooth(&points, 2.0), width, color);
    });
}

/// Emits a donut: each `(fraction, color)` is a share of the ring, starting at the top.
#[track_caller]
pub fn donut_chart(modifier: Modifier, shares: Vec<(f32, Color)>, thickness: f32, track: Color) {
    canvas(modifier, move |canvas| {
        let side = canvas.width().min(canvas.height());
        let (cx, cy) = (canvas.width() / 2.0, canvas.height() / 2.0);
        let radius = side / 2.0 - thickness / 2.0;
        canvas.arc(cx, cy, radius, thickness, 0.0, 1.0, track);
        let mut start = 0.0;
        for (fraction, color) in &shares {
            canvas.arc(cx, cy, radius, thickness, start * 360.0, *fraction, *color);
            start += fraction;
        }
    });
}
