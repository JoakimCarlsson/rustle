//! A cyberpunk control room: a synthwave horizon behind neon-framed panels,
//! live streaming charts, tick-ring dials, a radar, a target lock, code rain, a
//! terminal, an oscilloscope, and canvases that redraw every frame as a heat
//! field and a net trace that reacts to the pointer.
//!
//! Run it with `cargo run --release --example advance`.

mod field;
mod fx;
mod hacker;
mod hud;
mod scene;
mod telemetry;

use rustle::prelude::*;

use fx::{CYAN, GREEN, NIGHT, PINK, PURPLE};
use hud::stat_tile;

/// A row of panels with the same gap between each.
#[composable]
fn band(content: impl FnOnce()) {
    row_arranged(
        Modifier::new().fill_max_width(),
        Arrangement::SpacedBy(16.0),
        Alignment::Start,
        content,
    );
    spacer(Modifier::new().height(16.0));
}

#[composable]
fn app() {
    let scheme = ColorScheme {
        background: NIGHT,
        ..dark_color_scheme()
    };
    theme(scheme, None, None, || {
        stack_with(Modifier::new().fill_max_size(), || {
            scene::backdrop_scene();
            let scroll = remember_scroll_state();
            column_with(
                Modifier::new().fill_max_size().vertical_scroll(&scroll),
                || {
                    column_with(Modifier::new().fill_max_width().padding(28.0), || {
                        scene::header();
                        spacer(Modifier::new().height(18.0));
                        band(|| {
                            stat_tile(
                                "Netrunners",
                                48_213.0,
                                0,
                                "",
                                "▲ 12.4% this cycle",
                                CYAN,
                                0.3,
                            );
                            stat_tile(
                                "Intrusion rate",
                                6.84,
                                2,
                                "%",
                                "▲ 0.8% this cycle",
                                PINK,
                                1.7,
                            );
                            stat_tile("Eddies", 912.4, 1, "k", "▲ 3.1% this cycle", PURPLE, 2.9);
                            stat_tile("Trace level", 0.12, 2, "%", "▼ 0.4% this cycle", GREEN, 4.1);
                        });
                        band(|| {
                            telemetry::stream_panel();
                            telemetry::dials_panel();
                        });
                        band(|| {
                            hacker::radar_panel();
                            field::heat_field_panel();
                            hacker::reticle_panel();
                        });
                        band(|| {
                            hacker::rain_panel();
                            hacker::terminal_panel();
                            hacker::scope_panel();
                        });
                        band(|| {
                            field::constellation_panel();
                            field::spectrum_panel();
                            telemetry::breakdown_panel();
                        });
                        hacker::ticker();
                    });
                },
            );
        });
    });
}

fn main() {
    rustle::run(app);
}
