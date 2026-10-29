use std::cell::Cell;
use std::rc::Rc;

use rustle_foundation::Modifier;

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
