//! An animated counter with bars.

use rustle::prelude::*;

use super::{linear, noise};
use crate::ui::{money, panel};

#[composable]
pub(super) fn counter_and_bars() {
    let theme = theme();
    let total = remember(|| 15_231.89_f64);
    let seed = remember(|| 7_u64);
    let shown = animate_float_as_state(total.get() as f32, linear(900, Easing::EaseOut)).get();

    let mut heights = Vec::new();
    for i in 0..12_u64 {
        let target = 0.18 + noise(seed.get() * 31 + i) * 0.82;
        heights.push(key(i, || animate_float_as_state(target, spring()).get()));
    }
    let (strong, faint) = (theme.primary, theme.accent);
    panel(
        "Tweened values",
        "Numbers and bars glide to their new value instead of jumping.",
        || {
            title(&money(f64::from(shown)), 28.0);
            muted_sized("Total revenue", 12.0);
            spacer(Modifier::new().height(12.0));
            canvas(
                Modifier::new().fill_max_width().height(96.0),
                move |canvas| {
                    let count = heights.len();
                    let gap = 8.0;
                    let width = (canvas.width() - gap * (count as f32 - 1.0)) / count as f32;
                    for (i, height) in heights.iter().enumerate() {
                        let h = (height * canvas.height()).clamp(2.0, canvas.height());
                        let x = i as f32 * (width + gap);
                        canvas.rounded_rect(x, 0.0, width, canvas.height(), 5.0, faint);
                        canvas.rounded_rect(x, canvas.height() - h, width, h, 5.0, strong);
                    }
                },
            );
            spacer(Modifier::new().height(12.0));
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(8.0),
                Alignment::Center,
                || {
                    let (total, seed) = (total.clone(), seed.clone());
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Default,
                        "Shuffle",
                        Some(phosphor::PULSE),
                        None,
                        move || {
                            seed.update(|s| *s += 1);
                            total.set(1_000.0 + f64::from(noise(seed.get() * 7)) * 90_000.0);
                        },
                    );
                },
            );
        },
    );
}
