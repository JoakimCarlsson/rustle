use rustle_foundation::Canvas;
use rustle_graphics::TextAlign;

use crate::style::*;

/// A smooth curve through `points` (left to right) that never overshoots them,
/// as a list of points close enough together to draw as lines.
pub(crate) fn smooth(points: &[(f32, f32)], step: f32) -> Vec<(f32, f32)> {
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
pub(crate) fn value_range(series: &[Series], style: ChartStyle) -> (f32, f32) {
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
pub(crate) fn nice_maximum(series: &[Series]) -> f32 {
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

pub(crate) fn grid_and_labels(
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
