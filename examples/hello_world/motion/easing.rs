//! Easing curves and springs raced side by side.

use rustle::prelude::*;

use super::{linear, replay_button};
use crate::ui::panel;

#[composable]
pub(super) fn track(label: &str, spec: AnimationSpec, end: bool) {
    let theme = color_scheme();
    let progress = animate_float_as_state(if end { 1.0 } else { 0.0 }, spec).get();
    let (line, dot) = (theme.outline, theme.primary);
    row_arranged(
        Modifier::new().fill_max_width().padding_xy(0.0, 3.0),
        Arrangement::SpacedBy(8.0),
        Alignment::Center,
        || {
            stack_with(Modifier::new().width(84.0), || muted_sized(label, 12.0));
            canvas(Modifier::new().weight(1.0).height(24.0), move |canvas| {
                let (left, right) = (10.0, canvas.width() - 10.0);
                let x = left + progress * (right - left);
                let middle = canvas.height() / 2.0;
                canvas.line((left, middle), (right, middle), 2.0, line);
                canvas.line((left, middle), (x, middle), 2.0, dot);
                canvas.circle(x, middle, 8.0, dot);
            });
        },
    );
}

#[composable]
pub(super) fn easing_race() {
    let end = remember(|| false);
    panel(
        "Easing and springs",
        "The same trip, taken five ways. The spring overshoots.",
        || {
            let tracks = [
                ("Linear", linear(1100, Easing::Linear)),
                ("Ease in", linear(1100, Easing::EaseIn)),
                ("Ease out", linear(1100, Easing::EaseOut)),
                ("Ease in-out", linear(1100, Easing::EaseInOut)),
                ("Spring", spring()),
            ];
            for (index, (label, spec)) in tracks.into_iter().enumerate() {
                key(index, || track(label, spec, end.get()));
            }
            spacer(Modifier::new().height(10.0));
            let toggle = end.clone();
            replay_button("Run it", move || toggle.update(|end| *end = !*end));
        },
    );
}
