//! Tabs with a sliding indicator.

use rustle::prelude::*;

use crate::ui::panel;

#[composable]
pub(super) fn sliding_tabs() {
    let theme = theme();
    const WIDTH: f32 = 96.0;
    let labels = ["Overview", "Analytics", "Reports"];
    let selected = remember(|| 0_usize);
    let x = animate_float_as_state(selected.get() as f32 * WIDTH, spring()).get();
    panel(
        "Sliding tabs",
        "The highlight springs to the tab you pick; the content crossfades.",
        || {
            stack_aligned(
                Modifier::new()
                    .background_rounded(theme.secondary, 10.0)
                    .border(1.0, theme.outline, 10.0)
                    .padding(3.0),
                BoxAlignment::CenterStart,
                || {
                    spacer(
                        Modifier::new()
                            .offset(x, 0.0)
                            .size(WIDTH, 30.0)
                            .shadow(3.0, 8.0)
                            .background_rounded(theme.accent, 8.0),
                    );
                    row_arranged(
                        Modifier::new(),
                        Arrangement::Start,
                        Alignment::Center,
                        || {
                            for (index, label) in labels.iter().enumerate() {
                                key(index, || {
                                    let pick = selected.clone();
                                    stack_aligned(
                                        Modifier::new()
                                            .size(WIDTH, 30.0)
                                            .semantics(
                                                Semantics::new(Role::Tab)
                                                    .label(label.to_string())
                                                    .toggled(selected.get() == index)
                                                    .merging_descendants(),
                                            )
                                            .clickable(move || pick.set(index)),
                                        BoxAlignment::Center,
                                        || {
                                            let color = if selected.get() == index {
                                                theme.on_surface
                                            } else {
                                                theme.on_surface_muted
                                            };
                                            text_styled(
                                                Modifier::new(),
                                                label.to_string(),
                                                LOCAL_TEXT_STYLE
                                                    .current()
                                                    .color(color)
                                                    .font_weight(FontWeight::MEDIUM),
                                            );
                                        },
                                    );
                                });
                            }
                        },
                    );
                },
            );
            spacer(Modifier::new().height(14.0));
            stack_with(Modifier::new().fill_max_width().height(64.0), || {
                crossfade(selected.get(), |index| {
                    column(|| match index {
                        0 => {
                            title("Welcome back", 15.0);
                            muted_sized("Everything at a glance, as of this morning.", 13.0);
                        }
                        1 => {
                            title("Traffic is up 12%", 15.0);
                            muted_sized("Most of it came from two referrals.", 13.0);
                        }
                        _ => {
                            title("3 reports ready", 15.0);
                            muted_sized("Share them, or schedule the next run.", 13.0);
                        }
                    });
                });
            });
        },
    );
}
