//! Spreads a trackpad's scrolls evenly over the frames that show them.
//!
//! A trackpad sends its scrolls, and the momentum after the fingers lift, at about the rate
//! of the display, but not in step with its refreshes: each scroll arrives a little early or
//! late. Applied as they arrive, a refresh that comes just before a scroll shows no motion
//! and the next shows two scrolls' worth, which reads as the view about to stop and then
//! catching up. [`WheelResampler`] instead treats the scrolls as samples of where the fingers
//! have moved the content and, at each frame, moves it to where the fingers were a few
//! milliseconds before, between the two scrolls around that moment, as browsers resample
//! touch and scroll input. Every scroll is still applied in full, a scroll's interval later.

use std::collections::VecDeque;
use std::time::Instant;

use rustle_geometry::Offset;

/// The least a frame waits behind the scrolls it shows, in seconds.
const LEAST_DELAY: f64 = 0.006;

/// The most a frame waits behind the scrolls it shows, in seconds.
const MOST_DELAY: f64 = 0.03;

/// How much later than a scroll's usual interval a frame samples it, in seconds: enough that
/// one arriving a little late is already in.
const DELAY_MARGIN: f64 = 0.004;

/// The interval between scrolls assumed at the start of a stream, in seconds: a 120 Hz
/// trackpad's.
const FIRST_INTERVAL: f64 = 1.0 / 120.0;

/// How long without a scroll ends a stream of them, in seconds, so the next one starts anew.
const STREAM_GAP: f64 = 0.06;

/// How much of the difference between when a scroll arrived and when its interval says it
/// was due counts: arrivals jitter with the run loop, while the trackpad samples evenly.
const ARRIVAL_GAIN: f64 = 0.3;

/// How much each new interval between scrolls moves the interval counted on.
const INTERVAL_GAIN: f64 = 0.2;

/// Scrolls in pixels received, to be handed out over the frames as [`take`](Self::take)
/// samples them.
#[derive(Debug, Default)]
pub(crate) struct WheelResampler {
    /// When the current stream of scrolls started; times are counted in seconds from it.
    origin: Option<Instant>,
    /// When the latest scroll arrived, in seconds from `origin`.
    arrived: f64,
    /// The interval between scrolls counted on, in seconds.
    interval: f64,
    /// How far the scrolls of the current stream had moved at the moment each was taken to
    /// be made, in seconds from `origin`, the first the start of the stream; older ones are
    /// dropped once sampled past.
    points: VecDeque<(f64, Offset)>,
    /// How far the scrolls received have moved in all.
    total: Offset,
    /// How far the scrolls handed out have moved in all.
    taken: Offset,
}

impl WheelResampler {
    /// Takes in a scroll by `delta` that arrived at `at`.
    ///
    /// The moment it counts as made is the one its interval after the scroll before
    /// predicts, moved part of the way to when it arrived, so jitter in arrival does not
    /// show as jitter in speed while a trackpad slower or faster than counted on is followed.
    pub(crate) fn push(&mut self, at: Instant, delta: Offset) {
        let since = self
            .origin
            .map(|origin| at.saturating_duration_since(origin).as_secs_f64());
        let fresh =
            self.points.is_empty() || since.is_none_or(|since| since - self.arrived > STREAM_GAP);
        if fresh {
            self.origin = Some(at);
            self.arrived = 0.0;
            self.interval = FIRST_INTERVAL;
            self.points.clear();
            self.points.push_back((-FIRST_INTERVAL, self.total));
            self.total = add(self.total, delta);
            self.points.push_back((0.0, self.total));
            return;
        }
        let since = since.unwrap_or(0.0);
        let gap = (since - self.arrived).clamp(0.002, 0.04);
        self.interval += (gap - self.interval) * INTERVAL_GAIN;
        self.arrived = since;
        let last = self.points.back().map_or(0.0, |(when, _)| *when);
        let due = last + self.interval;
        let made = (due + (since - due) * ARRIVAL_GAIN).max(last + 0.001);
        self.total = add(self.total, delta);
        self.points.push_back((made, self.total));
    }

    /// How far behind a frame the scrolls it shows are sampled, in seconds: a little more
    /// than the interval between them.
    fn delay(&self) -> f64 {
        (self.interval + DELAY_MARGIN).clamp(LEAST_DELAY, MOST_DELAY)
    }

    /// The part of the scrolls received that a frame drawn at `frame` shows: how far they
    /// had moved a little more than a scroll's interval before it, less what earlier frames
    /// showed.
    pub(crate) fn take(&mut self, frame: Instant) -> Offset {
        let Some(origin) = self.origin else {
            return self.drain();
        };
        let sample = frame.saturating_duration_since(origin).as_secs_f64() - self.delay();
        let position = self.position_at(sample);
        while self.points.len() > 2 && self.points[1].0 <= sample {
            self.points.pop_front();
        }
        let delta = sub(position, self.taken);
        self.taken = position;
        delta
    }

    /// Hands out every scroll received not handed out yet, as before an event that must
    /// see them all applied.
    pub(crate) fn drain(&mut self) -> Offset {
        let delta = sub(self.total, self.taken);
        self.taken = self.total;
        self.points.clear();
        delta
    }

    /// Whether some of the scrolls received are not handed out yet, so another frame must
    /// come to show them.
    pub(crate) fn pending(&self) -> bool {
        self.taken != self.total
    }

    /// How far the scrolls had moved at `at`, in seconds from the stream's start, along the
    /// straight line between the two made around it: none of the stream before its start,
    /// all of it after its last.
    fn position_at(&self, at: f64) -> Offset {
        let Some(&(first_at, first)) = self.points.front() else {
            return self.total;
        };
        if at <= first_at {
            return first;
        }
        let Some(after) = self.points.iter().position(|(when, _)| *when > at) else {
            return self.total;
        };
        let (from_at, from) = self.points[after - 1];
        let (to_at, to) = self.points[after];
        let share = ((at - from_at) / (to_at - from_at)) as f32;
        Offset::new(
            from.x + (to.x - from.x) * share,
            from.y + (to.y - from.y) * share,
        )
    }
}

/// `a` and `b` added.
fn add(a: Offset, b: Offset) -> Offset {
    Offset::new(a.x + b.x, a.y + b.y)
}

/// `b` taken from `a`.
fn sub(a: Offset, b: Offset) -> Offset {
    Offset::new(a.x - b.x, a.y - b.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// One refresh of a 120 Hz display.
    const REFRESH: f64 = 1.0 / 120.0;

    /// A small generator of numbers in -1..1, the same each run.
    struct Jitter(u64);

    impl Jitter {
        /// The next number.
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
        }
    }

    /// A trackpad scroll up and its momentum: scrolls every `interval` seconds, each
    /// arriving up to `jitter` seconds early or late, `step` pixels at first, the momentum
    /// after `held` scrolls decaying by `decay` a scroll. Returns when each arrived, in
    /// seconds, with its pixels.
    fn gesture(interval: f64, jitter: f64, step: f32, held: usize, decay: f32) -> Vec<(f64, f32)> {
        let mut noise = Jitter(7);
        let mut scrolls = Vec::new();
        let mut pixels = step;
        for index in 0..held + 200 {
            if index >= held {
                pixels *= decay;
            }
            if pixels.abs() < 0.5 {
                break;
            }
            let at = 0.1 + index as f64 * interval + noise.next() * jitter;
            scrolls.push((at, pixels));
        }
        scrolls.sort_by(|a, b| a.0.total_cmp(&b.0));
        scrolls
    }

    /// What each frame of a display refreshing every [`REFRESH`] seconds, its first at
    /// `phase`, shows of `scrolls`, applied as they arrive when `resample` is off and
    /// through a [`WheelResampler`] when it is on.
    fn frames(scrolls: &[(f64, f32)], phase: f64, resample: bool) -> Vec<f32> {
        let origin = Instant::now();
        let time = |seconds: f64| origin + Duration::from_secs_f64(seconds);
        let mut resampler = WheelResampler::default();
        let mut shown = Vec::new();
        let mut next = 0;
        let end = scrolls.last().map_or(0.0, |last| last.0) + 0.05;
        let mut frame = phase;
        while frame < end {
            let mut arrived = 0.0;
            while next < scrolls.len() && scrolls[next].0 <= frame {
                resampler.push(time(scrolls[next].0), Offset::new(0.0, scrolls[next].1));
                arrived += scrolls[next].1;
                next += 1;
            }
            shown.push(if resample {
                resampler.take(time(frame)).y
            } else {
                arrived
            });
            frame += REFRESH;
        }
        shown
    }

    /// How many frames, between two that move at least `floor` pixels each, show less
    /// than half or more than one and a half times what the frames either side show on
    /// average: a frame that seems to stop, or one that catches up.
    fn uneven(shown: &[f32], floor: f32) -> usize {
        shown
            .windows(3)
            .filter(|three| {
                let around = (three[0] + three[2]) / 2.0;
                three[0].abs() >= floor
                    && three[2].abs() >= floor
                    && !(0.5..=1.5).contains(&(three[1] / around))
            })
            .count()
    }

    /// Scrolls at the display's rate, arriving up to two milliseconds off, applied as they
    /// arrive give frames that stop and catch up; resampled, every frame moves about as far
    /// as the ones around it, through the momentum too, at every phase of the display, and
    /// all of the scroll is applied.
    #[test]
    fn resampled_scrolls_move_every_frame_evenly() {
        let scrolls = gesture(REFRESH, 0.002, -12.0, 40, 0.95);
        let total: f32 = scrolls.iter().map(|scroll| scroll.1).sum();
        let mut naive = 0;
        for tenth in 0..10 {
            let phase = 0.1 + REFRESH * tenth as f64 / 10.0;
            naive += uneven(&frames(&scrolls, phase, false), 3.0);
            let shown = frames(&scrolls, phase, true);
            assert_eq!(uneven(&shown, 3.0), 0, "phase {tenth}: {shown:?}");
            let applied: f32 = shown.iter().sum();
            assert!((applied - total).abs() < 0.01, "{applied} of {total}");
        }
        assert!(naive > 20, "{naive}");
    }

    /// A trackpad sending scrolls a little faster or slower than the display refreshes
    /// still moves every frame evenly once resampled.
    #[test]
    fn scrolls_off_the_display_rate_move_every_frame_evenly() {
        for interval in [1.0 / 125.0, 1.0 / 115.0, 1.0 / 90.0] {
            let scrolls = gesture(interval, 0.001, -10.0, 60, 0.94);
            let shown = frames(&scrolls, 0.1, true);
            assert_eq!(uneven(&shown, 2.0), 0, "{interval}: {shown:?}");
        }
    }

    /// Draining hands out what is left at once, and a scroll after a pause starts a new
    /// stream, spread over [`FIRST_INTERVAL`].
    #[test]
    fn draining_and_new_streams_apply_everything() {
        let origin = Instant::now();
        let mut resampler = WheelResampler::default();
        resampler.push(origin, Offset::new(0.0, 10.0));
        assert!(resampler.pending());
        assert_eq!(resampler.drain(), Offset::new(0.0, 10.0));
        assert!(!resampler.pending());
        let later = origin + Duration::from_secs(1);
        resampler.push(later, Offset::new(0.0, 8.0));
        let delay = Duration::from_secs_f64(FIRST_INTERVAL + DELAY_MARGIN);
        let half = resampler.take(later + delay - Duration::from_secs_f64(FIRST_INTERVAL / 2.0));
        assert!((half.y - 4.0).abs() < 0.01, "{half:?}");
        let rest = resampler.take(later + delay);
        assert!((rest.y - 4.0).abs() < 0.01, "{rest:?}");
        assert!(!resampler.pending());
    }
}
