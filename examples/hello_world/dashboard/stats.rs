//! The stat cards above the chart.

use rustle::prelude::*;

use super::Metric;

#[composable]
pub(super) fn delta_pill(label: &str, up: bool) {
    let theme = color_scheme();
    row_arranged(
        Modifier::new()
            .border(1.0, theme.outline, 100.0)
            .padding_xy(8.0, 2.0),
        Arrangement::SpacedBy(4.0),
        Alignment::Center,
        || {
            icon(
                if up {
                    phosphor::TREND_UP
                } else {
                    phosphor::TREND_DOWN
                },
                12.0,
                theme.on_surface,
            );
            let style = LOCAL_TEXT_STYLE
                .current()
                .font_size(12.0)
                .font_weight(FontWeight::MEDIUM);
            text_styled(Modifier::new(), label.to_string(), style);
        },
    );
}

#[composable]
pub(super) fn stat_card(metric: &Metric, selected: bool, on_click: impl Fn() + 'static) {
    let theme = color_scheme();
    card(
        Modifier::new()
            .weight(1.0)
            .semantics(
                Semantics::new(Role::Button)
                    .label(metric.label.to_string())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .border_shape(
                1.5,
                if selected {
                    theme.on_surface_variant
                } else {
                    Color::TRANSPARENT
                },
                shapes().medium,
            ),
        || {
            column_with(Modifier::new().fill_max_width().padding(20.0), || {
                row_arranged(
                    Modifier::new().fill_max_width(),
                    Arrangement::SpaceBetween,
                    Alignment::Center,
                    || {
                        muted(metric.label);
                        delta_pill(metric.delta, metric.up);
                    },
                );
                spacer(Modifier::new().height(8.0));
                title(metric.value, 28.0);
                spacer(Modifier::new().height(14.0));
                row_arranged(
                    Modifier::new(),
                    Arrangement::SpacedBy(6.0),
                    Alignment::Center,
                    || {
                        title(metric.headline, 13.0);
                        icon(
                            if metric.up {
                                phosphor::TREND_UP
                            } else {
                                phosphor::TREND_DOWN
                            },
                            14.0,
                            theme.on_surface,
                        );
                    },
                );
                muted_sized(metric.note, 13.0);
            });
        },
    );
}
