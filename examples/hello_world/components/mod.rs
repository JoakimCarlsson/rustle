//! A catalogue of controls, each doing something real.

mod first;
mod second;
mod third;

use rustle::prelude::*;

use crate::ctx::Ctx;
use first::first_column;
use second::second_column;
use third::third_column;

pub const ACCOUNTS: [(&str, &str); 3] = [
    ("Main Checking", "-8402"),
    ("High Yield Savings", "-1192"),
    ("Brokerage", "-3307"),
];

#[composable]
pub fn components_page(ctx: &Ctx) {
    row_arranged(
        Modifier::new().fill_max_width().padding(24.0),
        Arrangement::SpacedBy(16.0),
        Alignment::Start,
        || {
            column_with(Modifier::new().weight(1.0), || first_column(ctx));
            column_with(Modifier::new().weight(1.0), || second_column(ctx));
            column_with(Modifier::new().weight(1.0), || third_column(ctx));
        },
    );
}
