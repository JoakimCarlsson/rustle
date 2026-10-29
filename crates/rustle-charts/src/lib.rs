//! Charts and a QR code, drawn on a canvas in whatever colors they are given.

use std::cell::Cell;
use std::rc::Rc;

use qrcode::{Color as Module, QrCode};
use rustle_foundation::{Canvas, Modifier, canvas};
use rustle_graphics::{Color, TextAlign};

/// How the axes and labels of a chart look.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartStyle {
    /// Horizontal guide lines.
    pub grid: Color,
    /// Axis labels.
    pub label: Color,
    pub label_size: f32,
    /// The color behind the chart, for the holes in markers.
    pub surface: Color,
    /// Start the value axis at zero. Off, it starts at the lowest value, so
    /// small changes fill the height.
    pub zero_baseline: bool,
}

impl Default for ChartStyle {
    fn default() -> Self {
        Self {
            grid: Color::rgba(255, 255, 255, 18),
            label: Color::rgb(113, 113, 122),
            label_size: 11.0,
            surface: Color::rgb(17, 17, 19),
            zero_baseline: true,
        }
    }
}

/// One line of a chart, with its values from left to right.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub values: Vec<f32>,
    pub color: Color,
    /// Fill the space under the line with a fading tint of `color`.
    pub fill: bool,
    pub width: f32,
}

impl Series {
    pub fn new(values: Vec<f32>, color: Color) -> Self {
        Self {
            values,
            color,
            fill: false,
            width: 1.5,
        }
    }

    /// Fills the space under the line.
    pub fn filled(mut self) -> Self {
        self.fill = true;
        self
    }

    pub fn thickness(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

/// A smooth curve through `points` (left to right) that never overshoots them,
/// as a list of points close enough together to draw as lines.
fn smooth(points: &[(f32, f32)], step: f32) -> Vec<(f32, f32)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let slopes: Vec<f32> = points
        .windows(2)
        .map(|pair| (pair[1].1 - pair[0].1) / (pair[1].0 - pair[0].0).max(0.0001))
        .collect();
    let mut tangents = vec![0.0; points.len()];
    tangents[0] = slopes[0];
    tangents[points.len() - 1] = slopes[slopes.len() - 1];
    for index in 1..points.len() - 1 {
        let (before, after) = (slopes[index - 1], slopes[index]);
        tangents[index] = if before * after <= 0.0 {
            0.0
        } else {
            2.0 * before * after / (before + after)
        };
    }
    let mut curve = vec![points[0]];
    for index in 0..points.len() - 1 {
        let ((x0, y0), (x1, y1)) = (points[index], points[index + 1]);
        let width = x1 - x0;
        let samples = (width / step).ceil().max(1.0) as usize;
        for sample in 1..=samples {
            let t = sample as f32 / samples as f32;
            let (t2, t3) = (t * t, t * t * t);
            let y = (2.0 * t3 - 3.0 * t2 + 1.0) * y0
                + (t3 - 2.0 * t2 + t) * width * tangents[index]
                + (-2.0 * t3 + 3.0 * t2) * y1
                + (t3 - t2) * width * tangents[index + 1];
            curve.push((x0 + width * t, y));
        }
    }
    curve
}

/// The values at the bottom and the top of the value axis for `series`.
fn value_range(series: &[Series], style: ChartStyle) -> (f32, f32) {
    let all = || {
        series
            .iter()
            .flat_map(|series| series.values.iter().copied())
    };
    if style.zero_baseline {
        return (0.0, nice_maximum(series));
    }
    let (low, high) = (
        all().fold(f32::MAX, f32::min),
        all().fold(f32::MIN, f32::max),
    );
    let pad = ((high - low) * 0.1).max(0.0001);
    (low - pad, high + pad)
}

/// The largest value among `series`, rounded up to a tidy number.
fn nice_maximum(series: &[Series]) -> f32 {
    let largest = series
        .iter()
        .flat_map(|series| series.values.iter().copied())
        .fold(0.0_f32, f32::max)
        .max(0.0001);
    let magnitude = 10.0_f32.powf(largest.log10().floor());
    let scaled = largest / magnitude;
    let step = if scaled <= 1.0 {
        1.0
    } else if scaled <= 2.0 {
        2.0
    } else if scaled <= 5.0 {
        5.0
    } else {
        10.0
    };
    step * magnitude
}

fn grid_and_labels(
    canvas: &mut Canvas,
    style: ChartStyle,
    area: (f32, f32, f32, f32),
    labels: &[String],
    label_every: usize,
    slot_centered: bool,
) {
    let (left, top, width, height) = area;
    for line in 0..=3 {
        let y = top + height * line as f32 / 3.0;
        canvas.line((left, y), (left + width, y), 1.0, style.grid);
    }
    let count = labels.len();
    for (index, label) in labels.iter().enumerate() {
        if label_every > 1 && index % label_every != 0 {
            continue;
        }
        let slot = width / count.max(1) as f32;
        // A label is centered on its value and as wide as it needs, not as its slot.
        let box_width = 72.0;
        let center = if slot_centered {
            left + slot * index as f32 + slot / 2.0
        } else if count > 1 {
            left + width * index as f32 / (count - 1) as f32
        } else {
            left
        };
        let x = (center - box_width / 2.0).clamp(left - 8.0, left + width + 8.0 - box_width);
        canvas.text(
            x,
            top + height + 8.0,
            box_width,
            label,
            style.label_size,
            400,
            style.label,
            TextAlign::Center,
        );
    }
}

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

/// Emits a QR code for `data`, `size` square, with `dark` modules on a `light` background.
#[track_caller]
pub fn qr_code(size: f32, data: &str, dark: Color, light: Color) {
    let modules: Option<(usize, Vec<bool>)> = QrCode::new(data.as_bytes()).ok().map(|code| {
        (
            code.width(),
            code.to_colors()
                .iter()
                .map(|m| *m == Module::Dark)
                .collect(),
        )
    });
    canvas(Modifier::new().size(size, size), move |canvas| {
        canvas.rounded_rect(0.0, 0.0, size, size, 6.0, light);
        let Some((width, dark_modules)) = &modules else {
            return;
        };
        // A quiet border of two modules around the code.
        let cell = size / (*width as f32 + 4.0);
        for (index, is_dark) in dark_modules.iter().enumerate() {
            if *is_dark {
                let (column, row) = (index % width, index / width);
                canvas.rect(
                    (column as f32 + 2.0) * cell,
                    (row as f32 + 2.0) * cell,
                    cell + 0.4,
                    cell + 0.4,
                    dark,
                );
            }
        }
    });
}

/// A modifier that tracks which of `count` evenly spaced values the pointer is
/// nearest, between `left` and `right_inset` from the chart's edges, and the
/// index it is nearest right now, if the pointer is over the chart.
#[track_caller]
pub fn chart_hover(count: usize, left: f32, right_inset: f32) -> (Modifier, Option<usize>) {
    rustle_runtime::group(|| {
        let width = rustle_runtime::remember(|| Rc::new(Cell::new(1.0_f32))).get();
        let hovered = rustle_runtime::remember(|| None::<usize>);
        let (measure, track) = (Rc::clone(&width), hovered.clone());
        let modifier = Modifier::new()
            .on_size_changed(move |size| measure.set(size.width))
            .on_pointer_move(move |at| {
                let next = at.map(|point| {
                    let plot = (width.get() - left - right_inset).max(1.0);
                    let t = ((point.x - left) / plot).clamp(0.0, 1.0);
                    (t * count.saturating_sub(1).max(1) as f32).round() as usize
                });
                if track.get() != next {
                    track.set(next);
                }
            });
        (modifier, hovered.get())
    })
}

/// Like [`chart_hover`], for bars: the index of the bar slot the pointer is over.
#[track_caller]
pub fn bar_hover(count: usize) -> (Modifier, Option<usize>) {
    rustle_runtime::group(|| {
        let width = rustle_runtime::remember(|| Rc::new(Cell::new(1.0_f32))).get();
        let hovered = rustle_runtime::remember(|| None::<usize>);
        let (measure, track) = (Rc::clone(&width), hovered.clone());
        let modifier = Modifier::new()
            .on_size_changed(move |size| measure.set(size.width))
            .on_pointer_move(move |at| {
                let next = at.map(|point| {
                    let slot = width.get() / count.max(1) as f32;
                    ((point.x / slot).floor().max(0.0) as usize).min(count.saturating_sub(1))
                });
                if track.get() != next {
                    track.set(next);
                }
            });
        (modifier, hovered.get())
    })
}
