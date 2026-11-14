//! Layout parity with Compose: intrinsics, custom layouts, constraints-aware
//! composition, constraint layout and flow layouts.

use rustle::prelude::*;
use rustle::{
    Anchor, BarrierDirection, ChainStyle, ConstraintSet, Dimension, GuidePosition, Offset,
    Placement, Size, constraint_layout,
};

use crate::ui::panel;

/// A small filled chip used as a stand-in child.
#[composable]
fn block(label: &str) {
    let theme = theme();
    stack_aligned(
        Modifier::new()
            .background_rounded(theme.accent, 6.0)
            .padding_xy(10.0, 6.0),
        BoxAlignment::Center,
        || {
            text_styled(
                Modifier::new(),
                label.to_string(),
                LOCAL_TEXT_STYLE.current().font_size(12.0),
            );
        },
    );
}

/// Shows `width_intrinsic`, `Modifier::layout`, `with_constraints` and flow rows.
#[composable]
pub(super) fn layouts() {
    panel(
        "Intrinsic sizes",
        "A column as wide as its widest child, so the dividers match.",
        || {
            column_with(Modifier::new().width_intrinsic(IntrinsicSize::Max), || {
                block("Short");
                spacer(
                    Modifier::new()
                        .fill_max_width()
                        .height(1.0)
                        .background(theme().outline),
                );
                block("A much longer label");
            });
        },
    );
    panel(
        "Custom layout",
        "Modifier::layout measures with its own constraints and places the content.",
        || {
            stack_with(
                Modifier::new().layout(|measurable, constraints| {
                    let size = measurable.measure(constraints);
                    Placement::new(
                        Size::new(size.width + 24.0, size.height + 24.0),
                        Offset::new(24.0, 24.0),
                    )
                }),
                || block("Shifted by a layout modifier"),
            );
        },
    );
    panel(
        "With constraints",
        "The content is composed while measuring, from what room it gets.",
        || {
            with_constraints(Modifier::new().fill_max_width(), |constraints| {
                if constraints.max_width < 420.0 {
                    column(|| {
                        block("Narrow: stacked");
                        block("Second");
                    });
                } else {
                    row(|| {
                        block("Wide: side by side");
                        block("Second");
                    });
                }
            });
        },
    );
    panel(
        "Constraint layout",
        "Guideline, barrier and a chain.",
        || {
            let mut set = ConstraintSet::new();
            let title = set.child();
            let subtitle = set.child();
            let first = set.child();
            let second = set.child();
            let guide = set.vertical_guideline(GuidePosition::Fraction(0.3));
            let barrier = set.barrier(BarrierDirection::Bottom, &[title, subtitle]);
            set.constrain(title)
                .link_start(Anchor::start_of(guide).margin(8.0))
                .link_top(Anchor::PARENT_START.margin(8.0));
            set.constrain(subtitle)
                .link_start(Anchor::start_of(title))
                .link_top(Anchor::end_of(title).margin(4.0));
            set.constrain(first)
                .link_start(Anchor::PARENT_START)
                .link_end(Anchor::start_of(second))
                .link_top(Anchor::end_of(barrier).margin(12.0))
                .width(Dimension::Fill);
            set.constrain(second)
                .link_start(Anchor::end_of(first))
                .link_end(Anchor::PARENT_END)
                .link_top(Anchor::end_of(barrier).margin(12.0));
            set.horizontal_chain(&[first, second], ChainStyle::SpreadInside);
            constraint_layout(Modifier::new().fill_max_width().height(120.0), set, || {
                block("Title");
                block("Subtitle");
                block("Chain one");
                block("Chain two");
            });
        },
    );
    panel(
        "Flow row",
        "Two lines at most; the rest are left out.",
        || {
            flow_row_with(
                Modifier::new().fill_max_width().width(260.0),
                FlowOptions::new(6.0, 6.0).max_lines(2),
                || {
                    for word in [
                        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta",
                        "iota", "kappa", "lambda", "mu", "nu", "xi", "omicron",
                    ] {
                        key(word, || block(word));
                    }
                },
            );
        },
    );
}
