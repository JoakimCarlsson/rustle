//! A small benchmark harness for Rustle, built on the standard library alone.
//!
//! Each benchmark runs a closure a number of times after a few warm-up runs and prints
//! the median, the 95th percentile, the slowest run and the mean. The benches under
//! `benches/` use it; run them with
//!
//! ```sh
//! CARGO_TARGET_DIR=build cargo bench -p rustle-bench
//! CARGO_TARGET_DIR=build cargo bench -p rustle-bench --bench text -- streamed
//! ```
//!
//! Words given after `--` keep only the benchmarks whose names contain one of them.

use std::hint::black_box;

use rustle_graphics::{Color, FontFamily, StyledSpan};
use std::time::{Duration, Instant};

/// How many runs of a benchmark are made, and thrown away, before it is measured.
const WARM_UP: usize = 3;

/// The times one benchmark took.
#[derive(Clone, Debug)]
pub struct Stats {
    /// What the benchmark is called.
    pub name: String,
    /// The time of each measured run, sorted from fastest to slowest.
    pub samples: Vec<Duration>,
}

impl Stats {
    /// The time below which `percent` of the runs finished.
    pub fn percentile(&self, percent: usize) -> Duration {
        let at = (self.samples.len() * percent).div_ceil(100).max(1) - 1;
        self.samples[at.min(self.samples.len() - 1)]
    }

    /// The median run.
    pub fn median(&self) -> Duration {
        self.percentile(50)
    }

    /// The mean of the runs.
    pub fn mean(&self) -> Duration {
        let total: Duration = self.samples.iter().sum();
        total / self.samples.len().max(1) as u32
    }

    /// The sum of every run.
    pub fn total(&self) -> Duration {
        self.samples.iter().sum()
    }
}

/// Whether the benchmark named `name` is kept by the words given on the command line
/// after `--`: every benchmark when none are given.
pub fn selected(name: &str) -> bool {
    let words: Vec<String> = std::env::args()
        .skip(1)
        .filter(|word| !word.starts_with('-'))
        .collect();
    words.is_empty() || words.iter().any(|word| name.contains(word.as_str()))
}

/// Runs `run` `iterations` times, each given the number of the run counting from nought,
/// after [`WARM_UP`] runs that are not measured, and prints and returns its times. Returns
/// `None` without running it when the command line leaves it out.
///
/// Warm-up runs are given numbers past the measured ones, so a benchmark that sets
/// different text each run does not hit what a warm-up left cached.
pub fn bench<T>(name: &str, iterations: usize, mut run: impl FnMut(usize) -> T) -> Option<Stats> {
    if !selected(name) {
        return None;
    }
    for warm in 0..WARM_UP {
        black_box(run(iterations + warm));
    }
    let mut samples = Vec::with_capacity(iterations);
    for index in 0..iterations {
        let started = Instant::now();
        black_box(run(index));
        samples.push(started.elapsed());
    }
    samples.sort();
    let stats = Stats {
        name: name.to_owned(),
        samples,
    };
    report(&stats);
    Some(stats)
}

/// Runs `run` once per step for `steps` steps, measuring each, with no warm-up: for a
/// sequence whose steps depend on each other, such as a paragraph that grows as it streams
/// in, where `setup` makes the state each pass starts from. Prints and returns the times,
/// or `None` when the command line leaves it out.
pub fn bench_sequence<S, T>(
    name: &str,
    steps: usize,
    setup: impl FnOnce() -> S,
    mut run: impl FnMut(&mut S, usize) -> T,
) -> Option<Stats> {
    if !selected(name) {
        return None;
    }
    let mut state = setup();
    let mut samples = Vec::with_capacity(steps);
    for step in 0..steps {
        let started = Instant::now();
        black_box(run(&mut state, step));
        samples.push(started.elapsed());
    }
    samples.sort();
    let stats = Stats {
        name: name.to_owned(),
        samples,
    };
    report(&stats);
    Some(stats)
}

/// Prints one line for `stats`: the name, the median, 95th percentile, slowest and mean
/// run in microseconds, the total in milliseconds and how many runs were measured.
pub fn report(stats: &Stats) {
    let micros = |duration: Duration| duration.as_secs_f64() * 1_000_000.0;
    println!(
        "{:<44} p50 {:>10.1}us  p95 {:>10.1}us  max {:>10.1}us  mean {:>10.1}us  total {:>9.2}ms  n={}",
        stats.name,
        micros(stats.median()),
        micros(stats.percentile(95)),
        micros(*stats.samples.last().unwrap_or(&Duration::ZERO)),
        micros(stats.mean()),
        stats.total().as_secs_f64() * 1000.0,
        stats.samples.len()
    );
}

/// A deterministic run of English-looking words, `count` long, for text benchmarks: the
/// same seed gives the same words.
pub fn words(count: usize, seed: u64) -> Vec<&'static str> {
    const WORDS: [&str; 32] = [
        "the",
        "renderer",
        "shapes",
        "every",
        "paragraph",
        "once",
        "while",
        "streaming",
        "text",
        "grows",
        "word",
        "by",
        "and",
        "fades",
        "in",
        "at",
        "its",
        "end",
        "so",
        "layout",
        "must",
        "stay",
        "cheap",
        "for",
        "long",
        "replies",
        "with",
        "code",
        "links",
        "bold",
        "spans",
        "frames",
    ];
    let mut state = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    (0..count)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            WORDS[(state >> 33) as usize % WORDS.len()]
        })
        .collect()
}

/// How many runs the newest characters of a streamed paragraph fade in over.
pub const FADE_RUNS: usize = 8;

/// How many characters each fading run holds.
pub const FADE_RUN_CHARS: usize = 6;

/// The colour running text is set in.
pub const INK: Color = Color::rgb(220, 220, 228);

/// A plain run of `text` in the running colour.
pub fn plain(text: impl Into<String>) -> StyledSpan {
    StyledSpan::new(text, INK, 400, FontFamily::SansSerif)
}

/// The runs of a paragraph of `count` words as markdown sets them: running text with a
/// bold word every twelve words and a link every thirty, so a long reply holds a run for
/// every few words.
pub fn markdown(count: usize, seed: u64) -> Vec<StyledSpan> {
    let mut spans = Vec::new();
    let mut running = String::new();
    for (index, word) in words(count, seed).into_iter().enumerate() {
        let gap = if index == 0 { "" } else { " " };
        if index % 12 == 5 || index % 30 == 17 {
            running.push_str(gap);
            spans.push(plain(std::mem::take(&mut running)));
            let mut span = plain(word);
            if index % 12 == 5 {
                span.font_weight = 600;
            } else {
                span.color = Color::rgb(120, 170, 255);
                span.underline = true;
            }
            spans.push(span);
        } else {
            running.push_str(gap);
            running.push_str(word);
        }
    }
    if !running.is_empty() {
        spans.push(plain(running));
    }
    spans.retain(|span| !span.text.is_empty());
    spans
}

/// `spans` with their last [`FADE_RUNS`] runs of [`FADE_RUN_CHARS`] characters split off
/// and drawn ever fainter toward the end, as text streaming in fades in; `phase` moves the
/// fade on, as each frame does.
pub fn faded(spans: &[StyledSpan], phase: usize) -> Vec<StyledSpan> {
    let text: String = spans.iter().map(|span| span.text.as_str()).collect();
    let fading = (FADE_RUNS * FADE_RUN_CHARS).min(text.len());
    let mut cut = text.len() - fading;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut out = Vec::with_capacity(spans.len() + FADE_RUNS);
    let mut start = 0;
    for span in spans {
        let end = start + span.text.len();
        if end <= cut {
            out.push(span.clone());
        } else if start < cut {
            let mut head = span.clone();
            head.text = span.text[..cut - start].to_owned();
            out.push(head);
        }
        start = end;
    }
    let tail = &text[cut..];
    let mut offset = 0;
    for run in 0..FADE_RUNS {
        if offset >= tail.len() {
            break;
        }
        let mut end = (offset + FADE_RUN_CHARS).min(tail.len());
        while !tail.is_char_boundary(end) {
            end += 1;
        }
        let alpha = 255 - ((run * 28 + phase * 3) % 230) as u8;
        out.push(StyledSpan::new(
            &tail[offset..end],
            Color::rgba(INK.r, INK.g, INK.b, alpha),
            400,
            FontFamily::SansSerif,
        ));
        offset = end;
    }
    out
}

/// The runs of `spans` that hold its first `count` words.
pub fn prefix(spans: &[StyledSpan], count: usize) -> Vec<StyledSpan> {
    let mut seen = 0;
    let mut out = Vec::new();
    for span in spans {
        let mut cut = span.text.len();
        for (offset, character) in span.text.char_indices() {
            if character == ' ' && offset > 0 {
                seen += 1;
                if seen >= count {
                    cut = offset;
                    break;
                }
            }
        }
        let mut kept = span.clone();
        kept.text.truncate(cut);
        out.push(kept);
        if seen >= count {
            break;
        }
    }
    out
}
