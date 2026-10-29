//! Drag and drop between columns.

use rustle::prelude::*;

use crate::ctx::Ctx;
use crate::ui::panel;

const LANES: [&str; 3] = ["To do", "In progress", "Done"];

#[composable]
pub(super) fn kanban(ctx: &Ctx) {
    let theme = theme();
    let cards = remember_state_list(|| {
        vec![
            (0_usize, "Write the docs", 0_usize),
            (1, "Review the pull request", 0),
            (2, "Fix the flaky test", 1),
            (3, "Ship 0.1", 2),
        ]
    });
    panel(
        "Drag and drop",
        "Pick a card up and drop it on another column.",
        || {
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpacedBy(12.0),
                Alignment::Start,
                || {
                    for (lane, name) in LANES.iter().enumerate() {
                        key(lane, || {
                            let (cards, ctx) = (cards.clone(), ctx.clone());
                            let dropped = cards.clone();
                            column_with(
                                Modifier::new()
                                    .weight(1.0)
                                    .height_in(150.0, 400.0)
                                    .drop_target(move |payload| {
                                        let Some(id) = payload.downcast_ref::<usize>() else {
                                            return;
                                        };
                                        let list = dropped.to_vec();
                                        if let Some((_, label, from)) =
                                            list.iter().find(|(card, _, _)| card == id)
                                            && *from != lane
                                        {
                                            ctx.say(format!(
                                                "Moved \u{201c}{label}\u{201d} to {}",
                                                LANES[lane]
                                            ));
                                        }
                                        dropped.update(|list| {
                                            for card in list.iter_mut().filter(|c| c.0 == *id) {
                                                card.2 = lane;
                                            }
                                        });
                                    })
                                    .background_rounded(theme.secondary, 10.0)
                                    .border(1.0, theme.outline, 10.0)
                                    .padding(10.0),
                                || {
                                    row_arranged(
                                        Modifier::new().fill_max_width().padding_xy(2.0, 4.0),
                                        Arrangement::SpaceBetween,
                                        Alignment::Center,
                                        || {
                                            title(name, 13.0);
                                            let count = cards
                                                .to_vec()
                                                .iter()
                                                .filter(|c| c.2 == lane)
                                                .count();
                                            pill(&count.to_string(), BadgeStyle::Secondary);
                                        },
                                    );
                                    spacer(Modifier::new().height(6.0));
                                    for (id, label, _) in
                                        cards.to_vec().into_iter().filter(|c| c.2 == lane)
                                    {
                                        key(id, || {
                                            row_arranged(
                                                Modifier::new()
                                                    .fill_max_width()
                                                    .drag_source(id)
                                                    .background_rounded(theme.surface, 8.0)
                                                    .border(1.0, theme.outline, 8.0)
                                                    .padding_xy(10.0, 9.0),
                                                Arrangement::SpacedBy(8.0),
                                                Alignment::Center,
                                                || {
                                                    icon(
                                                        phosphor::DOTS_SIX_VERTICAL,
                                                        14.0,
                                                        theme.on_surface_muted,
                                                    );
                                                    text(label.to_string());
                                                },
                                            );
                                            spacer(Modifier::new().height(6.0));
                                        });
                                    }
                                },
                            );
                        });
                    }
                },
            );
        },
    );
}
