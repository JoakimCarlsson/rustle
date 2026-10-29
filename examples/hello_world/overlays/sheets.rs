//! Side sheets for reports and row details.

use rustle::prelude::*;

use crate::ctx::{Ctx, REVIEWERS};
use crate::ui::{icon_box, key_value, money};

#[composable]
pub(super) fn report_sheet(ctx: &Ctx) {
    let theme = theme();
    column_with(Modifier::new().fill_max_width().padding(24.0), || {
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceBetween,
            Alignment::Center,
            || {
                column(|| {
                    title("Full report", 18.0);
                    muted_sized("Contributions from December to May.", 13.0);
                });
                let ctx = ctx.clone();
                icon_box(phosphor::X, 30.0, "Close", move || ctx.close());
            },
        );
        spacer(Modifier::new().height(16.0));
        for (month, amount) in [
            ("December", 5200.0),
            ("January", 7400.0),
            ("February", 6600.0),
            ("March", 8200.0),
            ("April", 5600.0),
            ("May", 7800.0),
        ] {
            key(month, || key_value(month, &money(amount), false));
        }
        separator();
        key_value("Total", &money(40_800.0), true);
        spacer(Modifier::new().height(16.0));
        row_arranged(
            Modifier::new(),
            Arrangement::SpacedBy(8.0),
            Alignment::Center,
            || {
                let save = ctx.clone();
                styled_button(
                    Modifier::new(),
                    ButtonStyle::Default,
                    "Download PDF",
                    Some(phosphor::FILE),
                    None,
                    move || {
                        save.close();
                        save.say("Report saved as contributions.pdf");
                    },
                );
                let ctx = ctx.clone();
                styled_button(
                    Modifier::new(),
                    ButtonStyle::Outline,
                    "Email me a copy",
                    Some(phosphor::ENVELOPE_SIMPLE),
                    None,
                    move || {
                        ctx.close();
                        ctx.say("Report sent to m@example.com");
                    },
                );
            },
        );
        let _ = theme;
    });
}

#[composable]
pub(super) fn detail_sheet(ctx: &Ctx, id: usize) {
    let Some(doc) = ctx.doc(id) else {
        return;
    };
    column_with(Modifier::new().fill_max_width().padding(24.0), || {
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceBetween,
            Alignment::Center,
            || {
                column(|| {
                    title(&doc.header, 18.0);
                    muted_sized(
                        &format!("{} \u{2022} reviewed by {}", doc.kind, doc.reviewer),
                        13.0,
                    );
                });
                let ctx = ctx.clone();
                icon_box(phosphor::X, 30.0, "Close", move || ctx.close());
            },
        );
        spacer(Modifier::new().height(16.0));
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpacedBy(16.0),
            Alignment::Start,
            || {
                for (label, value, field) in [("Target", doc.target, 0), ("Limit", doc.limit, 1)] {
                    key(field, || {
                        column_with(Modifier::new().weight(1.0), || {
                            muted_sized(label, 12.0);
                            spacer(Modifier::new().height(6.0));
                            row_arranged(
                                Modifier::new(),
                                Arrangement::SpacedBy(12.0),
                                Alignment::Center,
                                || {
                                    let down = ctx.clone();
                                    icon_box(phosphor::CARET_LEFT, 30.0, "Decrease", move || {
                                        down.change_doc(id, |d| {
                                            let v = if field == 0 {
                                                &mut d.target
                                            } else {
                                                &mut d.limit
                                            };
                                            *v = v.saturating_sub(1);
                                        })
                                    });
                                    title(&value.to_string(), 20.0);
                                    let up = ctx.clone();
                                    icon_box(phosphor::CARET_RIGHT, 30.0, "Increase", move || {
                                        up.change_doc(id, |d| {
                                            let v = if field == 0 {
                                                &mut d.target
                                            } else {
                                                &mut d.limit
                                            };
                                            *v += 1;
                                        })
                                    });
                                },
                            );
                        });
                    });
                }
                column_with(Modifier::new().weight(1.0), || {
                    muted_sized("Reviewer", 12.0);
                    spacer(Modifier::new().height(6.0));
                    let pick = ctx.clone();
                    select(
                        Modifier::new().fill_max_width(),
                        REVIEWERS.iter().map(|r| r.to_string()).collect(),
                        REVIEWERS
                            .iter()
                            .position(|r| *r == doc.reviewer)
                            .unwrap_or(0),
                        move |index| {
                            pick.change_doc(id, |d| d.reviewer = REVIEWERS[index].to_string())
                        },
                    );
                });
            },
        );
        spacer(Modifier::new().height(20.0));
        row_arranged(
            Modifier::new(),
            Arrangement::SpacedBy(8.0),
            Alignment::Center,
            || {
                let toggle = ctx.clone();
                styled_button(
                    Modifier::new(),
                    ButtonStyle::Default,
                    if doc.done { "Reopen" } else { "Mark as done" },
                    Some(phosphor::CHECK),
                    None,
                    move || {
                        toggle.change_doc(id, |d| d.done = !d.done);
                        toggle.say("Status updated");
                    },
                );
                let remove = ctx.clone();
                styled_button(
                    Modifier::new(),
                    ButtonStyle::Destructive,
                    "Delete",
                    None,
                    None,
                    move || {
                        remove.close();
                        remove.delete_docs(&[id]);
                        remove.say("Section deleted");
                    },
                );
            },
        );
    });
}
