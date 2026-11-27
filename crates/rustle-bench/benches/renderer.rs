//! Renderer benchmarks: splitting a frame's draw commands into layers and shaping the text
//! they hold, without preparing or drawing anything on the GPU, for a list of rows and for
//! a paragraph streamed in a few words a frame. It needs a GPU adapter to make the renderer
//! and skips its benchmarks when there is none.
//!
//! ```sh
//! CARGO_TARGET_DIR=build cargo bench -p rustle-bench --bench renderer
//! ```

use rustle::prelude::*;
use rustle::{Embedded, TextAlign, TextWrap};
use rustle_bench::{bench, bench_sequence, faded, markdown, prefix};
use rustle_graphics::DrawCommand;
use rustle_wgpu::Renderer;

/// The window the frame is drawn in, in physical pixels.
const WINDOW: (u32, u32) = (900, 700);

/// How many words the streamed paragraph gains a frame.
const WORDS_PER_FRAME: usize = 3;

/// How many frames the streamed paragraph is measured over.
const STREAM_FRAMES: usize = 400;

/// The draw commands of a list of 600 rows, each a background, a title, a subtitle and a
/// trailing value, laid out in [`WINDOW`].
fn rows() -> Vec<DrawCommand> {
    let mut embedded = Embedded::without_renderer(
        || {
            column(|| {
                for index in 0..600 {
                    row_with(
                        Modifier::new()
                            .fill_max_width()
                            .height(40.0)
                            .background_rounded(Color::rgb(30, 30, 36), 6.0)
                            .padding(8.0),
                        || {
                            text(format!("Item {index}"));
                            spacer(Modifier::new().weight(1.0));
                            text("subtitle goes here");
                            text(format!("{}", index * 7 % 100));
                        },
                    );
                }
            });
        },
        || {},
    );
    embedded.resize(WINDOW, 1.0);
    embedded.update();
    embedded.commands().into_owned()
}

/// Runs every renderer benchmark.
fn main() {
    let mut renderer = match Renderer::offscreen(WINDOW.0, WINDOW.1) {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("renderer benchmarks skipped: no renderer could be made ({error})");
            return;
        }
    };
    let commands = rows();
    bench("renderer/layers_600_rows", 300, |_| {
        renderer.build_layers(&commands, 1.0)
    });
    let full = markdown(WORDS_PER_FRAME * STREAM_FRAMES, 4);
    let streamed: Vec<DrawCommand> = (0..STREAM_FRAMES)
        .map(|step| DrawCommand::RichText {
            x: 10.0,
            y: 10.0,
            width: 640.0,
            spans: faded(&prefix(&full, (step + 1) * WORDS_PER_FRAME), step),
            font_size: 14.0,
            line_height: 20.0,
            align: TextAlign::Start,
            max_lines: None,
            wrap: TextWrap::Word,
        })
        .collect();
    bench_sequence(
        "renderer/layers_streamed_paragraph",
        STREAM_FRAMES,
        || (),
        |(), step| renderer.build_layers(std::slice::from_ref(&streamed[step]), 1.0),
    );
}
