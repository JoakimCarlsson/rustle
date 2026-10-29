//! An auto-advancing carousel.

use std::time::Duration;

use rustle::prelude::*;

use crate::ui::panel;

#[composable]
pub(super) fn carousel() {
    let pager = remember_pager_state();
    let auto = pager.clone();
    launched_effect((), move |scope| async move {
        loop {
            scope.delay(Duration::from_millis(3200)).await;
            auto.animate_to_page((auto.current_page() + 1) % 3);
        }
    });
    panel(
        "Carousel",
        "Drag to swipe, or wait: it advances by itself.",
        || {
            let slides = [
                (
                    "Ship faster",
                    "Compose, recompose, done.",
                    Color::rgb(79, 70, 229),
                    Color::rgb(168, 85, 247),
                ),
                (
                    "Look sharp",
                    "Hairlines, tokens and a calm palette.",
                    Color::rgb(14, 116, 144),
                    Color::rgb(52, 211, 153),
                ),
                (
                    "Stay smooth",
                    "Springs, tweens and clocks.",
                    Color::rgb(190, 18, 60),
                    Color::rgb(251, 146, 60),
                ),
            ];
            horizontal_pager(
                Modifier::new().fill_max_width().height(150.0),
                &pager,
                slides.len(),
                |index| {
                    let (heading, detail, from, to) = slides[index];
                    stack_aligned(
                        Modifier::new()
                            .fill_max_size()
                            .background_gradient(from, to, 35.0, 12.0)
                            .padding(20.0),
                        BoxAlignment::BottomStart,
                        || {
                            column(|| {
                                let white = Color::rgb(255, 255, 255);
                                text_styled(
                                    Modifier::new(),
                                    heading.to_string(),
                                    LOCAL_TEXT_STYLE
                                        .current()
                                        .color(white)
                                        .font_size(24.0)
                                        .font_weight(FontWeight::BOLD),
                                );
                                text_styled(
                                    Modifier::new(),
                                    detail.to_string(),
                                    LOCAL_TEXT_STYLE
                                        .current()
                                        .color(Color::rgba(255, 255, 255, 210)),
                                );
                            });
                        },
                    );
                },
            );
            spacer(Modifier::new().height(12.0));
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::Center,
                Alignment::Center,
                || page_indicator(slides.len(), pager.current_page()),
            );
        },
    );
}
