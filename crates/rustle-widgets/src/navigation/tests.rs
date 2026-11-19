//! Tests of the segmented buttons: their shapes and how a row of them is laid out.

use std::cell::RefCell;
use std::rc::Rc;

use rustle_foundation::{Modifier, canvas};
use rustle_geometry::{Offset, Rect, Size};
use rustle_text::{LineLayout, StyledSpan, TextLayout, TextMeasurer, TextStyle};
use rustle_ui::{composition, display_list};
use rustle_unit::Constraints;

use super::{SegmentedButtonDefaults, segmented_button, single_choice_segmented_button_row};
use crate::shape::{CornerSizes, Shape};

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

/// The first and last buttons keep the outer corners of the base shape; those between are square.
#[test]
fn item_shapes_round_the_ends() {
    let base = Shape::rounded(6.0);
    assert_eq!(
        SegmentedButtonDefaults::item_shape(0, 3, base),
        Shape::Rounded(CornerSizes::new(6.0, 0.0, 0.0, 6.0))
    );
    assert_eq!(
        SegmentedButtonDefaults::item_shape(1, 3, base),
        Shape::Rounded(CornerSizes::uniform(0.0))
    );
    assert_eq!(
        SegmentedButtonDefaults::item_shape(2, 3, base),
        Shape::Rounded(CornerSizes::new(0.0, 6.0, 6.0, 0.0))
    );
    assert_eq!(SegmentedButtonDefaults::item_shape(0, 1, base), base);
}

/// Neighbours overlap by the row's space and every button is as tall as the row.
#[test]
fn row_overlaps_neighbours() {
    let rects: Rc<RefCell<Vec<Rect>>> = Rc::default();
    let kept = Rc::clone(&rects);
    let mut composition = composition(move || {
        let kept = Rc::clone(&kept);
        single_choice_segmented_button_row(Modifier::new().height(32.0), 1.0, move || {
            for index in 0..3 {
                let kept = Rc::clone(&kept);
                segmented_button(
                    index == 1,
                    || {},
                    SegmentedButtonDefaults::item_shape(index, 3, Shape::rounded(6.0)),
                    Modifier::new().on_globally_positioned(move |rect| {
                        kept.borrow_mut().push(rect);
                    }),
                    true,
                    SegmentedButtonDefaults::colors(),
                    SegmentedButtonDefaults::border_stroke(
                        rustle_graphics::Color::TRANSPARENT,
                        1.0,
                    ),
                    (10.0, 0.0),
                    || {},
                    || canvas(Modifier::new().size(20.0, 10.0), |_| {}),
                );
            }
        });
    });
    composition.compose();
    let window = Constraints::loose(Size::new(300.0, 100.0));
    display_list(composition.output().unwrap(), window, &mut NoText);
    let mut found = rects.borrow().clone();
    found.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
    assert_eq!(found.len(), 3);
    for (index, rect) in found.iter().enumerate() {
        assert_eq!(rect.size, Size::new(40.0, 32.0));
        assert_eq!(rect.origin, Offset::new(index as f32 * 39.0, 0.0));
    }
}
