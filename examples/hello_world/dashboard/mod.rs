//! The metrics page: stat cards that pick what the chart shows, and a
//! document table that filters, sorts, selects and pages.

mod chart;
mod stats;
mod table;

use rustle::prelude::*;

use crate::ctx::Ctx;
use chart::visitors_chart;
use stats::stat_card;
use table::document_section;

struct Metric {
    label: &'static str,
    value: &'static str,
    delta: &'static str,
    up: bool,
    headline: &'static str,
    note: &'static str,
    names: (&'static str, &'static str),
    seed: f32,
    base: f32,
}

const METRICS: [Metric; 4] = [
    Metric {
        label: "Total Revenue",
        value: "$1,250.00",
        delta: "+12.5%",
        up: true,
        headline: "Trending up this month",
        note: "Visitors for the last 6 months",
        names: ("Mobile", "Desktop"),
        seed: 1.0,
        base: 230.0,
    },
    Metric {
        label: "New Customers",
        value: "1,234",
        delta: "-20%",
        up: false,
        headline: "Down 20% this period",
        note: "Acquisition needs attention",
        names: ("Referral", "Organic"),
        seed: 4.2,
        base: 140.0,
    },
    Metric {
        label: "Active Accounts",
        value: "45,678",
        delta: "+12.5%",
        up: true,
        headline: "Strong user retention",
        note: "Engagement exceed targets",
        names: ("Free", "Paid"),
        seed: 7.7,
        base: 300.0,
    },
    Metric {
        label: "Growth Rate",
        value: "4.5%",
        delta: "+4.5%",
        up: true,
        headline: "Steady performance increase",
        note: "Meets growth projections",
        names: ("Target", "Actual"),
        seed: 9.1,
        base: 90.0,
    },
];

#[composable]
pub fn dashboard_page(ctx: &Ctx) {
    let metric = remember(|| 0usize);
    column_with(Modifier::new().fill_max_width().padding(24.0), || {
        row_arranged(
            Modifier::new().fill_max_width(),
            Arrangement::SpacedBy(16.0),
            Alignment::Start,
            || {
                for (index, item) in METRICS.iter().enumerate() {
                    key(index, || {
                        let pick = metric.clone();
                        stat_card(item, metric.get() == index, move || pick.set(index));
                    });
                }
            },
        );
        spacer(Modifier::new().height(16.0));
        visitors_chart(&METRICS[metric.get().min(3)]);
        spacer(Modifier::new().height(16.0));
        document_section(ctx);
    });
}
