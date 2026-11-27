//! Text shaping and layout benchmarks: short labels, a long paragraph, a paragraph with a
//! run per word, and a paragraph streamed in a few words a frame with its newest words
//! fading in, the way a chat reply arrives.
//!
//! ```sh
//! CARGO_TARGET_DIR=build cargo bench -p rustle-bench --bench text
//! ```

use rustle_bench::{bench, bench_sequence, faded, markdown, plain, prefix, words};
use rustle_graphics::{Color, StyledSpan};
use rustle_text::{TextMeasurer, TextStyle};
use rustle_wgpu::TextMetrics;

/// How wide paragraphs are wrapped, in logical pixels.
const WIDTH: f32 = 640.0;

/// How many words a streamed paragraph gains a frame.
const WORDS_PER_FRAME: usize = 3;

/// How many frames a streamed paragraph is measured over.
const STREAM_FRAMES: usize = 400;

/// The style paragraphs are set in.
fn style() -> TextStyle {
    TextStyle::default()
}

/// Measures and lays out `spans` at `width` as a frame of an interface showing them does:
/// once to size the text and once for the layout its selection and drawing use.
fn frame(metrics: &mut TextMetrics, spans: &[StyledSpan], width: f32) -> usize {
    let size = metrics.measure_spans(spans, &style(), width);
    let layout = metrics.layout(spans, &style(), width);
    size.height as usize + layout.line_count()
}

/// Runs every text benchmark.
fn main() {
    let mut metrics = TextMetrics::new();
    let label = [plain("Save changes")];
    bench("text/short_label", 2000, |index| {
        metrics.measure_spans(&label, &style(), 200.0 + index as f32 * 0.01)
    });
    let long = markdown(800, 1)
        .into_iter()
        .map(|span| plain(span.text))
        .collect::<Vec<_>>();
    bench("text/long_paragraph_800_words", 200, |index| {
        frame(&mut metrics, &long, WIDTH + index as f32 * 0.01)
    });
    let marked = markdown(800, 2);
    bench("text/long_paragraph_markdown_spans", 200, |index| {
        frame(&mut metrics, &marked, WIDTH + index as f32 * 0.01)
    });
    let per_word: Vec<StyledSpan> = words(800, 3)
        .into_iter()
        .enumerate()
        .map(|(index, word)| {
            let mut span = plain(format!("{word} "));
            span.color = Color::rgb(100 + (index % 100) as u8, 200, 220);
            span
        })
        .collect();
    bench("text/many_spans_one_per_word", 100, |index| {
        frame(&mut metrics, &per_word, WIDTH + index as f32 * 0.01)
    });
    let full = markdown(WORDS_PER_FRAME * STREAM_FRAMES, 4);
    let streamed: Vec<Vec<StyledSpan>> = (0..STREAM_FRAMES)
        .map(|step| faded(&prefix(&full, (step + 1) * WORDS_PER_FRAME), step))
        .collect();
    bench_sequence(
        "text/streamed_paragraph",
        STREAM_FRAMES,
        TextMetrics::new,
        |metrics, step| frame(metrics, &streamed[step], WIDTH),
    );
    let earlier: Vec<StyledSpan> = (0..6)
        .flat_map(|seed| {
            let mut paragraph = markdown(120, 10 + seed);
            paragraph.push(plain("\n\n"));
            paragraph
        })
        .collect();
    let replies: Vec<Vec<StyledSpan>> = (0..STREAM_FRAMES)
        .map(|step| {
            let mut spans = earlier.clone();
            spans.extend(faded(
                &prefix(&full, (step + 1) * WORDS_PER_FRAME / 4 + 1),
                step,
            ));
            spans
        })
        .collect();
    bench_sequence(
        "text/streamed_reply_after_six_paragraphs",
        STREAM_FRAMES,
        TextMetrics::new,
        |metrics, step| frame(metrics, &replies[step], WIDTH),
    );
}
