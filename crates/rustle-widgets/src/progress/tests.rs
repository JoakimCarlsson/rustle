//! Tests of `linear_progress_indicator_with`: its size and what it draws.

use std::cell::Cell;
use std::rc::Rc;

use rustle_foundation::Modifier;
use rustle_geometry::{Rect, Size};
use rustle_text::{LineLayout, StyledSpan, TextLayout, TextMeasurer, TextStyle};
use rustle_ui::{composition, display_list};
use rustle_unit::Constraints;

use super::{
    ProgressIndicatorDefaults, circular_progress_indicator_with,
    circular_progress_indicator_with_progress, linear_progress_indicator_with,
};

/// A measurer for a tree without text.
struct NoText;

impl TextMeasurer for NoText {
    /// Text does not occur in these tests.
    fn measure(&mut self, _: &str, _: &TextStyle, _: f32) -> Size {
        Size::default()
    }

    /// Text does not occur in these tests.
    fn measure_spans(&mut self, _: &[StyledSpan], _: &TextStyle, _: f32) -> Size {
        Size::default()
    }

    /// Text does not occur in these tests.
    fn layout(&mut self, spans: &[StyledSpan], style: &TextStyle, _: f32) -> TextLayout {
        let line = LineLayout {
            start: 0,
            end: 0,
            top: 0.0,
            height: 0.0,
            baseline: 0.0,
            width: 0.0,
            rtl: false,
            hard_break: true,
            clusters: Vec::new(),
        };
        TextLayout::new(String::new(), spans.to_vec(), style.font_size, vec![line])
    }
}

/// Lays out an indicator with `modifier` and returns where it went.
fn placed(modifier: Modifier) -> Rect {
    let found = Rc::new(Cell::new(Rect::default()));
    let kept = Rc::clone(&found);
    let mut composition = composition(move || {
        let kept = Rc::clone(&kept);
        linear_progress_indicator_with(
            || 0.5,
            modifier
                .clone()
                .on_globally_positioned(move |rect| kept.set(rect)),
            rustle_graphics::Color::rgb(255, 0, 0),
            rustle_graphics::Color::rgb(0, 0, 0),
        );
    });
    composition.compose();
    let window = Constraints::loose(Size::new(400.0, 100.0));
    display_list(composition.output().unwrap(), window, &mut NoText);
    found.get()
}

/// Without a size of its own it takes Compose's default size; a size in its modifier wins.
#[test]
fn takes_default_size_unless_told() {
    assert_eq!(
        placed(Modifier::new()).size,
        Size::new(
            ProgressIndicatorDefaults::LINEAR_INDICATOR_WIDTH,
            ProgressIndicatorDefaults::LINEAR_INDICATOR_HEIGHT
        )
    );
    assert_eq!(
        placed(Modifier::new().size(100.0, 6.0)).size,
        Size::new(100.0, 6.0)
    );
}

/// The arcs a circular indicator made with `emit` draws, as (start, sweep, color red).
fn arcs(emit: impl Fn() + 'static) -> Vec<(f32, f32, u8)> {
    let mut composition = composition(emit);
    composition.compose();
    let window = Constraints::loose(Size::new(400.0, 100.0));
    let list = display_list(composition.output().unwrap(), window, &mut NoText);
    list.commands
        .iter()
        .filter_map(|command| match command {
            rustle_graphics::DrawCommand::Arc {
                start,
                sweep,
                color,
                ..
            } => Some((*start, *sweep, color.r)),
            _ => None,
        })
        .collect()
}

/// A determinate ring draws its track and then its share from the top; an indeterminate one
/// with no track draws only its arc.
#[test]
fn circular_indicators_draw_their_track_and_arc() {
    let drawn = arcs(|| {
        circular_progress_indicator_with_progress(
            || 0.25,
            Modifier::new(),
            rustle_graphics::Color::rgb(255, 0, 0),
            4.0,
            rustle_graphics::Color::rgb(1, 1, 1),
        );
    });
    assert_eq!(drawn, vec![(0.0, 1.0, 1), (0.0, 0.25, 255)]);
    let spinning = arcs(|| {
        circular_progress_indicator_with(
            Modifier::new(),
            rustle_graphics::Color::rgb(255, 0, 0),
            4.0,
            rustle_graphics::Color::TRANSPARENT,
        );
    });
    assert_eq!(spinning.len(), 1);
    assert_eq!(spinning[0].2, 255);
}
