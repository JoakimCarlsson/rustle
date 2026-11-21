//! Tests of the layouts of this crate against real measured rectangles.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_runtime::{State, remember};
use rustle_text::{LineLayout, StyledSpan, TextLayout, TextMeasurer, TextStyle};
use rustle_ui::{
    LOCAL_LAYOUT_DIRECTION, LayoutDirection, Placement, SubcomposeResult, canvas, composition,
    display_list, subcompose_layout,
};
use rustle_unit::Constraints;

use super::*;

/// The rectangles the probes of a test were placed at, by name.
type Rects = Rc<RefCell<HashMap<&'static str, Rect>>>;

/// A deterministic measurer: eight pixels per character, sixteen high.
struct FixedText;

impl TextMeasurer for FixedText {
    /// Returns eight pixels per character, ignoring the width.
    fn measure(&mut self, content: &str, _: &TextStyle, _: f32) -> Size {
        Size::new(content.chars().count() as f32 * 8.0, 16.0)
    }

    /// Measures the joined text of `spans`.
    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, width: f32) -> Size {
        let content: String = spans.iter().map(|span| span.text.as_str()).collect();
        self.measure(&content, style, width)
    }

    /// Returns one empty line; nothing here reads it.
    fn layout(&mut self, spans: &[StyledSpan], style: &TextStyle, _: f32) -> TextLayout {
        let line = LineLayout {
            start: 0,
            end: 0,
            top: 0.0,
            height: 16.0,
            baseline: 12.0,
            width: 0.0,
            rtl: false,
            hard_break: true,
            clusters: Vec::new(),
        };
        TextLayout::new(String::new(), spans.to_vec(), style.font_size, vec![line])
    }
}

/// Composes `root`, lays it out loosely in `window` and returns the rectangles probes recorded.
fn run(window: Size, rects: &Rects, root: impl FnMut() + 'static) -> HashMap<&'static str, Rect> {
    rects.borrow_mut().clear();
    let mut composition = composition(root);
    composition.compose();
    let output = composition.output().expect("composed");
    display_list(output, Constraints::loose(window), &mut FixedText);
    rects.borrow().clone()
}

/// A fresh set of rectangles.
fn rects() -> Rects {
    Rects::default()
}

/// Emits an empty area called `name` of the given size that records where it was placed.
fn probe(rects: &Rects, name: &'static str, width: f32, height: f32) {
    probe_with(rects, name, Modifier::new().size(width, height));
}

/// Emits an empty area called `name` with `modifier` that records where it was placed.
fn probe_with(rects: &Rects, name: &'static str, modifier: Modifier) {
    let rects = Rc::clone(rects);
    canvas(
        modifier.on_globally_positioned(move |rect| {
            rects.borrow_mut().insert(name, rect);
        }),
        |_| {},
    );
}

/// The rectangle as `(x, y, width, height)`.
fn tuple(rect: Rect) -> (f32, f32, f32, f32) {
    (
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    )
}

/// Lays out a 200 by 100 constraint layout of `count` probes `a`, `b`, ... of the given sizes.
fn constrained(
    set: ConstraintSet,
    sizes: &[(f32, f32)],
    window: (f32, f32),
) -> HashMap<&'static str, Rect> {
    const NAMES: [&str; 6] = ["a", "b", "c", "d", "e", "f"];
    let rects = rects();
    let sizes = sizes.to_vec();
    let kept = Rc::clone(&rects);
    run(Size::new(window.0, window.1), &rects, move || {
        let sizes = sizes.clone();
        let rects = Rc::clone(&kept);
        constraint_layout(
            Modifier::new().size(window.0, window.1),
            set.clone(),
            move || {
                for (index, (width, height)) in sizes.iter().enumerate() {
                    probe(&rects, NAMES[index], *width, *height);
                }
            },
        );
    })
}

/// A packed chain sits at its bias.
#[test]
fn packed_chain_follows_its_bias() {
    let mut set = ConstraintSet::new();
    let members = [set.child(), set.child(), set.child()];
    set.constrain(members[0])
        .link_start(Anchor::PARENT_START)
        .horizontal_bias(0.25);
    set.constrain(members[2]).link_end(Anchor::PARENT_END);
    set.horizontal_chain(&members, ChainStyle::Packed);
    let found = constrained(set, &[(20.0, 10.0); 3], (200.0, 100.0));
    assert_eq!(found["a"].origin.x, 35.0);
    assert_eq!(found["b"].origin.x, 55.0);
    assert_eq!(found["c"].origin.x, 75.0);
}

/// A spread chain leaves equal gaps, also at the ends.
#[test]
fn spread_chain_leaves_equal_gaps() {
    let mut set = ConstraintSet::new();
    let members = [set.child(), set.child(), set.child()];
    set.constrain(members[0]).link_start(Anchor::PARENT_START);
    set.constrain(members[2]).link_end(Anchor::PARENT_END);
    set.horizontal_chain(&members, ChainStyle::Spread);
    let found = constrained(set, &[(20.0, 10.0); 3], (200.0, 100.0));
    assert_eq!(found["a"].origin.x, 35.0);
    assert_eq!(found["b"].origin.x, 90.0);
    assert_eq!(found["c"].origin.x, 145.0);
}

/// Fill members of a chain share what the fixed ones leave, by weight.
#[test]
fn weighted_chain_shares_the_rest() {
    let mut set = ConstraintSet::new();
    let members = [set.child(), set.child(), set.child()];
    set.constrain(members[0])
        .link_start(Anchor::PARENT_START)
        .width(Dimension::Fill)
        .horizontal_weight(1.0);
    set.constrain(members[1]);
    set.constrain(members[2])
        .link_end(Anchor::PARENT_END)
        .width(Dimension::Fill)
        .horizontal_weight(3.0);
    set.horizontal_chain(&members, ChainStyle::Spread);
    let found = constrained(
        set,
        &[(1.0, 10.0), (20.0, 10.0), (1.0, 10.0)],
        (200.0, 100.0),
    );
    assert_eq!(tuple(found["a"]), (0.0, 0.0, 45.0, 10.0));
    assert_eq!(tuple(found["b"]), (45.0, 0.0, 20.0, 10.0));
    assert_eq!(tuple(found["c"]), (65.0, 0.0, 135.0, 10.0));
}

/// A vertical spread-inside chain runs from the top to the bottom edge.
#[test]
fn vertical_chain_spreads_inside() {
    let mut set = ConstraintSet::new();
    let members = [set.child(), set.child()];
    set.constrain(members[0]).link_top(Anchor::PARENT_START);
    set.constrain(members[1]).link_bottom(Anchor::PARENT_END);
    set.vertical_chain(&members, ChainStyle::SpreadInside);
    let found = constrained(set, &[(20.0, 10.0), (20.0, 30.0)], (200.0, 100.0));
    assert_eq!(found["a"].origin.y, 0.0);
    assert_eq!(found["b"].origin.y, 70.0);
}

/// A chain between two children works from where they ended up.
#[test]
fn chain_between_children_follows_them() {
    let mut set = ConstraintSet::new();
    let left = set.child();
    let right = set.child();
    let members = [set.child(), set.child()];
    set.constrain(left).link_start(Anchor::PARENT_START);
    set.constrain(right).link_end(Anchor::PARENT_END);
    set.constrain(members[0]).link_start(Anchor::end_of(left));
    set.constrain(members[1]).link_end(Anchor::start_of(right));
    set.horizontal_chain(&members, ChainStyle::SpreadInside);
    let found = constrained(
        set,
        &[(30.0, 10.0), (50.0, 10.0), (10.0, 10.0), (10.0, 10.0)],
        (200.0, 100.0),
    );
    assert_eq!(found["c"].origin.x, 30.0);
    assert_eq!(found["d"].origin.x, 140.0);
}

/// Guidelines from the start, the end and as a fraction sit where they say.
#[test]
fn guidelines_sit_where_declared() {
    let mut set = ConstraintSet::new();
    let (a, b, c) = (set.child(), set.child(), set.child());
    let top = set.horizontal_guideline(GuidePosition::Begin(30.0));
    let bottom = set.horizontal_guideline(GuidePosition::End(30.0));
    let middle = set.vertical_guideline(GuidePosition::Fraction(0.25));
    let end = set.vertical_guideline(GuidePosition::End(20.0));
    set.constrain(a)
        .link_top(Anchor::start_of(top))
        .link_start(Anchor::start_of(middle));
    set.constrain(b)
        .link_bottom(Anchor::start_of(bottom))
        .link_end(Anchor::start_of(end));
    set.constrain(c)
        .link_top(Anchor::end_of(top).margin(5.0))
        .link_bottom(Anchor::start_of(bottom).margin(5.0))
        .link_start(Anchor::PARENT_START)
        .link_end(Anchor::start_of(middle))
        .width(Dimension::Fill)
        .height(Dimension::Fill);
    let found = constrained(
        set,
        &[(10.0, 10.0), (10.0, 10.0), (1.0, 1.0)],
        (200.0, 100.0),
    );
    assert_eq!((found["a"].origin.x, found["a"].origin.y), (50.0, 30.0));
    assert_eq!((found["b"].origin.x, found["b"].origin.y), (170.0, 60.0));
    assert_eq!(tuple(found["c"]), (0.0, 35.0, 50.0, 30.0));
}

/// Barriers clear the member that reaches furthest in their direction.
#[test]
fn barriers_clear_every_member() {
    let mut set = ConstraintSet::new();
    let (a, b, below, right, above) = (
        set.child(),
        set.child(),
        set.child(),
        set.child(),
        set.child(),
    );
    set.constrain(a)
        .link_start(Anchor::PARENT_START.margin(10.0))
        .link_top(Anchor::PARENT_START.margin(40.0));
    set.constrain(b)
        .link_start(Anchor::PARENT_START.margin(50.0))
        .link_top(Anchor::PARENT_START.margin(20.0));
    let bottom = set.barrier(BarrierDirection::Bottom, &[a, b]);
    let end = set.barrier(BarrierDirection::End, &[a, b]);
    let top = set.barrier(BarrierDirection::Top, &[a, b]);
    let start = set.barrier(BarrierDirection::Start, &[a, b]);
    set.constrain(below).link_top(Anchor::end_of(bottom));
    set.constrain(right).link_start(Anchor::end_of(end));
    set.constrain(above)
        .link_bottom(Anchor::start_of(top))
        .link_end(Anchor::start_of(start));
    let found = constrained(
        set,
        &[
            (20.0, 30.0),
            (30.0, 15.0),
            (5.0, 5.0),
            (5.0, 5.0),
            (6.0, 4.0),
        ],
        (200.0, 100.0),
    );
    assert_eq!(found["c"].origin.y, 70.0);
    assert_eq!(found["d"].origin.x, 80.0);
    assert_eq!(found["e"].origin.y, 20.0 - 4.0);
    assert_eq!(found["e"].origin.x, 10.0 - 6.0);
}

/// A child anchored on both sides is centered, or placed by bias, inside the margins.
#[test]
fn both_anchors_center_or_bias() {
    let mut set = ConstraintSet::new();
    let (a, b) = (set.child(), set.child());
    set.constrain(a)
        .link_start(Anchor::PARENT_START)
        .link_end(Anchor::PARENT_END);
    set.constrain(b)
        .link_start(Anchor::PARENT_START.margin(10.0))
        .link_end(Anchor::PARENT_END.margin(30.0))
        .horizontal_bias(0.25)
        .link_top(Anchor::PARENT_START)
        .link_bottom(Anchor::PARENT_END)
        .vertical_bias(1.0);
    let found = constrained(set, &[(50.0, 10.0), (50.0, 10.0)], (200.0, 100.0));
    assert_eq!(found["a"].origin.x, 75.0);
    assert_eq!(found["b"].origin.x, 10.0 + 110.0 * 0.25);
    assert_eq!(found["b"].origin.y, 90.0);
}

/// A fixed dimension overrides what the child asks for.
#[test]
fn fixed_dimensions_win() {
    let mut set = ConstraintSet::new();
    let a = set.child();
    set.constrain(a)
        .width(Dimension::Fixed(60.0))
        .height(Dimension::Fixed(25.0));
    let found = constrained(set, &[(1.0, 1.0)], (200.0, 100.0));
    assert_eq!(tuple(found["a"]), (0.0, 0.0, 60.0, 25.0));
}

/// A layout without a size wraps what is anchored in it.
#[test]
fn a_wrapping_layout_takes_the_extent_of_its_children() {
    let rects = rects();
    let kept = Rc::clone(&rects);
    let mut set = ConstraintSet::new();
    let (a, b) = (set.child(), set.child());
    set.constrain(a)
        .link_start(Anchor::PARENT_START.margin(5.0));
    set.constrain(b).link_start(Anchor::end_of(a).margin(10.0));
    let found = run(Size::new(300.0, 300.0), &rects, move || {
        let inner = Rc::clone(&kept);
        let outer = Rc::clone(&kept);
        constraint_layout(
            Modifier::new().on_globally_positioned(move |rect| {
                outer.borrow_mut().insert("layout", rect);
            }),
            set.clone(),
            move || {
                probe(&inner, "a", 40.0, 10.0);
                probe(&inner, "b", 30.0, 25.0);
            },
        );
    });
    assert_eq!(found["b"].origin.x, 55.0);
    assert_eq!(found["layout"].size, Size::new(85.0, 25.0));
}

/// A child centered in a wrapping layout is not squeezed to half its width.
#[test]
fn a_centered_child_in_a_wrapping_layout_keeps_its_width() {
    let rects = rects();
    let kept = Rc::clone(&rects);
    let mut set = ConstraintSet::new();
    let a = set.child();
    set.constrain(a)
        .link_start(Anchor::PARENT_START.margin(4.0))
        .link_end(Anchor::PARENT_END.margin(6.0));
    let found = run(Size::new(300.0, 300.0), &rects, move || {
        let inner = Rc::clone(&kept);
        let outer = Rc::clone(&kept);
        constraint_layout(
            Modifier::new().on_globally_positioned(move |rect| {
                outer.borrow_mut().insert("layout", rect);
            }),
            set.clone(),
            move || probe(&inner, "a", 40.0, 10.0),
        );
    });
    assert_eq!(found["layout"].size.width, 50.0);
    assert_eq!(found["a"].origin.x, 4.0);
}

/// Constraint layouts do not mirror, but the fill child's height follows its width.
#[test]
fn fill_children_are_measured_at_their_solved_size() {
    let mut set = ConstraintSet::new();
    let a = set.child();
    set.constrain(a)
        .link_start(Anchor::PARENT_START.margin(10.0))
        .link_end(Anchor::PARENT_END.margin(10.0))
        .width(Dimension::Fill);
    let found = constrained(set, &[(1.0, 7.0)], (200.0, 100.0));
    assert_eq!(tuple(found["a"]), (10.0, 0.0, 180.0, 7.0));
}

/// Lays out a flow row or column of `sizes` in a bounded area and returns what was placed.
fn flowed(
    vertical: bool,
    window: (f32, f32),
    options: FlowOptions,
    sizes: &[(f32, f32)],
    weights: &[Option<f32>],
    direction: LayoutDirection,
) -> HashMap<&'static str, Rect> {
    const NAMES: [&str; 8] = ["a", "b", "c", "d", "e", "f", "g", "h"];
    let rects = rects();
    let kept = Rc::clone(&rects);
    let sizes = sizes.to_vec();
    let weights = weights.to_vec();
    run(Size::new(window.0, window.1), &rects, move || {
        let (rects, sizes, weights) = (Rc::clone(&kept), sizes.clone(), weights.clone());
        let outer = Rc::clone(&kept);
        LOCAL_LAYOUT_DIRECTION.provide(direction, || {
            let modifier = Modifier::new()
                .width(window.0)
                .on_globally_positioned(move |rect| {
                    outer.borrow_mut().insert("flow", rect);
                });
            let content = move || {
                for (index, (width, height)) in sizes.iter().enumerate() {
                    let mut modifier = Modifier::new().size(*width, *height);
                    if let Some(Some(weight)) = weights.get(index) {
                        modifier = modifier.weight(*weight);
                    }
                    probe_with(&rects, NAMES[index], modifier);
                }
            };
            if vertical {
                flow_column_with(modifier.height(window.1).width(window.0), options, content);
            } else {
                flow_row_with(modifier, options, content);
            }
        });
    })
}

/// A flow row wraps when the next child does not fit and spaces its lines.
#[test]
fn flow_row_wraps_with_spacing() {
    let found = flowed(
        false,
        (100.0, 200.0),
        FlowOptions::new(10.0, 5.0),
        &[(40.0, 10.0); 4],
        &[],
        LayoutDirection::Ltr,
    );
    assert_eq!(tuple(found["a"]), (0.0, 0.0, 40.0, 10.0));
    assert_eq!(tuple(found["b"]), (50.0, 0.0, 40.0, 10.0));
    assert_eq!(tuple(found["c"]), (0.0, 15.0, 40.0, 10.0));
    assert_eq!(tuple(found["d"]), (50.0, 15.0, 40.0, 10.0));
}

/// The tallest child of a line sets where the next line starts.
#[test]
fn flow_row_lines_are_as_tall_as_their_tallest() {
    let found = flowed(
        false,
        (100.0, 200.0),
        FlowOptions::new(0.0, 0.0),
        &[(60.0, 10.0), (30.0, 30.0), (50.0, 5.0)],
        &[],
        LayoutDirection::Ltr,
    );
    assert_eq!(found["c"].origin.y, 30.0);
    assert_eq!(found["b"].origin.x, 60.0);
}

/// Children past the line limit are not placed at all.
#[test]
fn flow_row_max_lines_drops_the_rest() {
    let found = flowed(
        false,
        (100.0, 200.0),
        FlowOptions::new(10.0, 5.0).max_lines(1),
        &[(40.0, 10.0); 4],
        &[],
        LayoutDirection::Ltr,
    );
    assert!(found.contains_key("a") && found.contains_key("b"));
    assert!(!found.contains_key("c") && !found.contains_key("d"));
    assert_eq!(found["flow"].size.height, 10.0);
}

/// A limit on children per line wraps early.
#[test]
fn flow_row_max_items_per_line() {
    let found = flowed(
        false,
        (200.0, 200.0),
        FlowOptions::new(0.0, 0.0).max_items_per_line(2),
        &[(20.0, 10.0); 5],
        &[],
        LayoutDirection::Ltr,
    );
    assert_eq!(found["c"].origin, Offset::new(0.0, 10.0));
    assert_eq!(found["e"].origin, Offset::new(0.0, 20.0));
}

/// Weighted children share what the other children leave of their line, each line on its
/// own, as Compose's `FlowRow` gives `Modifier.weight` children their share.
#[test]
fn flow_row_weights_share_each_line() {
    let found = flowed(
        false,
        (100.0, 200.0),
        FlowOptions::new(0.0, 0.0),
        &[
            (20.0, 10.0),
            (10.0, 10.0),
            (10.0, 10.0),
            (90.0, 10.0),
            (5.0, 10.0),
        ],
        &[None, Some(1.0), Some(3.0), None, Some(1.0)],
        LayoutDirection::Ltr,
    );
    assert_eq!(tuple(found["a"]), (0.0, 0.0, 20.0, 10.0));
    assert_eq!(tuple(found["b"]), (20.0, 0.0, 20.0, 10.0));
    assert_eq!(tuple(found["c"]), (40.0, 0.0, 60.0, 10.0));
    assert_eq!(tuple(found["d"]), (0.0, 10.0, 90.0, 10.0));
    assert_eq!(tuple(found["e"]), (90.0, 10.0, 10.0, 10.0));
}

/// With a limit on children per line, equally weighted children of different widths take
/// equal shares of each line, spacing left out, as tiles of Compose's `FlowRow` with
/// `maxItemsInEachRow` and `Modifier.weight(1f)` do; a weighted child is broken onto a line
/// by its least width.
#[test]
fn flow_row_weights_make_equal_columns() {
    let found = flowed(
        false,
        (110.0, 200.0),
        FlowOptions::new(10.0, 0.0).max_items_per_line(2),
        &[(30.0, 10.0), (60.0, 10.0), (20.0, 10.0)],
        &[Some(1.0), Some(1.0), Some(1.0)],
        LayoutDirection::Ltr,
    );
    assert_eq!(tuple(found["a"]), (0.0, 0.0, 50.0, 10.0));
    assert_eq!(tuple(found["b"]), (60.0, 0.0, 50.0, 10.0));
    assert_eq!(tuple(found["c"]), (0.0, 10.0, 110.0, 10.0));
}

/// A flow row in a right-to-left layout starts at the right edge.
#[test]
fn flow_row_mirrors_in_rtl() {
    let found = flowed(
        false,
        (100.0, 200.0),
        FlowOptions::new(10.0, 0.0),
        &[(40.0, 10.0); 3],
        &[],
        LayoutDirection::Rtl,
    );
    assert_eq!(found["a"].origin.x, found["flow"].origin.x + 100.0 - 40.0);
    assert_eq!(found["b"].origin.x, found["flow"].origin.x + 100.0 - 90.0);
    assert_eq!(found["c"].origin.x, found["flow"].origin.x + 100.0 - 40.0);
}

/// A flow column fills top to bottom, wraps into a new column and honors weights.
#[test]
fn flow_column_wraps_and_weights() {
    let found = flowed(
        true,
        (200.0, 100.0),
        FlowOptions::new(10.0, 5.0),
        &[(10.0, 40.0), (20.0, 40.0), (15.0, 40.0)],
        &[],
        LayoutDirection::Ltr,
    );
    assert_eq!(found["b"].origin, Offset::new(found["flow"].origin.x, 50.0));
    assert_eq!(found["c"].origin.y, found["flow"].origin.y);
    assert_eq!(found["c"].origin.x - found["flow"].origin.x, 20.0 + 5.0);

    let weighted = flowed(
        true,
        (200.0, 100.0),
        FlowOptions::new(0.0, 0.0),
        &[(10.0, 20.0), (10.0, 10.0), (10.0, 10.0)],
        &[None, Some(1.0), Some(3.0)],
        LayoutDirection::Ltr,
    );
    assert_eq!(weighted["b"].size.height, 20.0);
    assert_eq!(weighted["c"].size.height, 60.0);
    assert_eq!(weighted["c"].origin.y - weighted["flow"].origin.y, 40.0);
}

/// Columns of a flow column in a right-to-left layout start at the right edge.
#[test]
fn flow_column_mirrors_in_rtl() {
    let found = flowed(
        true,
        (100.0, 50.0),
        FlowOptions::new(0.0, 0.0),
        &[(30.0, 30.0), (30.0, 30.0)],
        &[],
        LayoutDirection::Rtl,
    );
    assert_eq!(found["a"].origin.x, found["flow"].origin.x + 100.0 - 30.0);
    assert_eq!(found["b"].origin.x, found["flow"].origin.x + 100.0 - 60.0);
}

/// `with_constraints` composes its content with the constraints and again when they change.
#[test]
fn with_constraints_follows_the_constraints() {
    let rects = rects();
    let kept = Rc::clone(&rects);
    let composed = Rc::new(RefCell::new(Vec::<f32>::new()));
    let seen = Rc::clone(&composed);
    let build = move |width: f32| {
        let (rects, seen) = (Rc::clone(&kept), Rc::clone(&seen));
        move || {
            let (rects, seen) = (Rc::clone(&rects), Rc::clone(&seen));
            with_constraints(Modifier::new().width(width), move |constraints| {
                seen.borrow_mut().push(constraints.max_width);
                if constraints.max_width > 100.0 {
                    probe(&rects, "wide", 10.0, 10.0);
                } else {
                    probe(&rects, "narrow", 10.0, 10.0);
                }
            });
        }
    };
    let found = run(Size::new(300.0, 300.0), &rects, build(150.0));
    assert!(found.contains_key("wide") && !found.contains_key("narrow"));
    let found = run(Size::new(300.0, 300.0), &rects, build(50.0));
    assert!(found.contains_key("narrow") && !found.contains_key("wide"));
    assert_eq!(*composed.borrow(), [150.0, 50.0]);
}

/// A `Modifier::layout` places its content at the offset and reports the placed rectangle.
#[test]
fn layout_modifier_offsets_and_reports() {
    let rects = rects();
    let kept = Rc::clone(&rects);
    let found = run(Size::new(300.0, 300.0), &rects, move || {
        let rects = Rc::clone(&kept);
        let outer = Rc::clone(&kept);
        column_with(Modifier::new().padding(5.0), move || {
            let rects = Rc::clone(&rects);
            let outer = Rc::clone(&outer);
            let modifier = Modifier::new()
                .on_placed(move |rect, window| {
                    outer.borrow_mut().insert("wrapper", rect);
                    assert_eq!(window, Size::new(300.0, 300.0));
                })
                .layout(|measurable, constraints| {
                    let size = measurable.measure(constraints);
                    Placement::new(
                        Size::new(size.width + 30.0, size.height + 10.0),
                        Offset::new(20.0, 10.0),
                    )
                });
            probe_with(&rects, "inner", modifier.size(40.0, 12.0));
        });
    });
    assert_eq!(tuple(found["wrapper"]), (5.0, 5.0, 70.0, 22.0));
    assert_eq!(tuple(found["inner"]), (25.0, 15.0, 40.0, 12.0));
}

/// Placement callbacks follow their node through nested paddings and offsets, and mirror in RTL.
#[test]
fn placement_callbacks_include_ancestors_and_rtl_offset() {
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        let rects = rects();
        let kept = Rc::clone(&rects);
        let found = run(Size::new(200.0, 100.0), &rects, move || {
            let rects = Rc::clone(&kept);
            LOCAL_LAYOUT_DIRECTION.provide(direction, || {
                let rects = Rc::clone(&rects);
                stack_with(Modifier::new().size(200.0, 100.0), move || {
                    let local = Rc::clone(&rects);
                    let absolute = Rc::clone(&rects);
                    probe_with(
                        &local,
                        "offset",
                        Modifier::new().offset(30.0, 7.0).size(10.0, 10.0),
                    );
                    probe_with(
                        &absolute,
                        "absolute",
                        Modifier::new().absolute_offset(30.0, 7.0).size(10.0, 10.0),
                    );
                });
            });
        });
        assert_eq!(found["absolute"].origin.y, 7.0);
        match direction {
            LayoutDirection::Ltr => {
                assert_eq!(found["offset"].origin.x, 30.0);
                assert_eq!(found["absolute"].origin.x, 30.0);
            }
            LayoutDirection::Rtl => {
                assert_eq!(found["offset"].origin.x, 200.0 - 10.0 - 30.0);
                assert_eq!(found["absolute"].origin.x, 200.0 - 10.0 + 30.0);
            }
        }
    }
}

/// Intrinsic sizes of text, rows, columns, flows and modifiers match what is measured.
#[test]
fn intrinsic_sizes_through_layouts() {
    let rects = rects();
    let kept = Rc::clone(&rects);
    let found = run(Size::new(400.0, 400.0), &rects, move || {
        let rects = Rc::clone(&kept);
        let record = |name: &'static str| {
            let rects = Rc::clone(&rects);
            Modifier::new().on_globally_positioned(move |rect| {
                rects.borrow_mut().insert(name, rect);
            })
        };
        column_with(Modifier::new(), || {
            row_with(record("row").width_intrinsic(IntrinsicSize::Max), || {
                text_with(record("t1"), "abcd");
                text_with(record("t2"), "ab");
            });
            column_with(
                record("column")
                    .width_intrinsic(IntrinsicSize::Max)
                    .padding(3.0),
                || {
                    text_with(record("short"), "abc");
                    text_with(record("long").fill_max_width(), "abcdefgh");
                },
            );
            flow_row_with(
                record("flow").width_intrinsic(IntrinsicSize::Max),
                FlowOptions::new(4.0, 0.0),
                || {
                    text("ab");
                    text("abc");
                },
            );
            flow_row_with(
                record("flow_min").width_intrinsic(IntrinsicSize::Min),
                FlowOptions::new(4.0, 0.0),
                || {
                    text("ab");
                    text("abcde");
                },
            );
        });
    });
    assert_eq!(found["row"].size.width, 48.0);
    assert_eq!(found["column"].size.width, 64.0 + 6.0);
    assert_eq!(found["long"].size.width, 64.0);
    assert_eq!(found["flow"].size.width, 16.0 + 4.0 + 24.0);
    assert_eq!(found["flow_min"].size.width, 40.0);
}

/// A subcompose layout recomposes its slots when the state they read changes.
#[test]
fn subcompose_slots_keep_state_and_follow_it() {
    let rects = rects();
    let kept = Rc::clone(&rects);
    let state = Rc::new(RefCell::new(None::<State<i32>>));
    let handle = Rc::clone(&state);
    let mut composition = composition(move || {
        let value = remember(|| 0);
        *handle.borrow_mut() = Some(value.clone());
        let rects = Rc::clone(&kept);
        subcompose_layout(Modifier::new().width(300.0), move |scope, constraints| {
            let (rects, number) = (Rc::clone(&rects), value.get());
            let indices = scope.subcompose("slot", move || {
                probe(&rects, "slot", 10.0 + number as f32, 10.0);
            });
            let loose = Constraints {
                min_width: 0.0,
                min_height: 0.0,
                ..constraints
            };
            let size = scope.measure(indices[0], loose);
            SubcomposeResult {
                size,
                placements: vec![(indices[0], Offset::new(7.0, 3.0))],
            }
        });
    });
    composition.compose();
    let window = Constraints::loose(Size::new(300.0, 300.0));
    display_list(composition.output().unwrap(), window, &mut FixedText);
    assert_eq!(tuple(rects.borrow()["slot"]), (7.0, 3.0, 10.0, 10.0));
    state.borrow().as_ref().unwrap().set(5);
    composition.recompose();
    display_list(composition.output().unwrap(), window, &mut FixedText);
    assert_eq!(tuple(rects.borrow()["slot"]), (7.0, 3.0, 15.0, 10.0));
}

/// Members of a chain may link to each other, as a chain is usually written; only the first
/// start and the last end bound it, and a fill member takes what the others leave.
#[test]
fn chain_members_may_link_to_each_other() {
    let mut set = ConstraintSet::new();
    let (first, second) = (set.child(), set.child());
    set.constrain(first)
        .link_start(Anchor::PARENT_START)
        .link_end(Anchor::start_of(second))
        .width(Dimension::Fill);
    set.constrain(second)
        .link_start(Anchor::end_of(first))
        .link_end(Anchor::PARENT_END);
    set.horizontal_chain(&[first, second], ChainStyle::SpreadInside);
    let found = constrained(set, &[(10.0, 10.0), (50.0, 10.0)], (200.0, 100.0));
    assert_eq!(tuple(found["a"]), (0.0, 0.0, 150.0, 10.0));
    assert_eq!(tuple(found["b"]), (150.0, 0.0, 50.0, 10.0));
}

/// `SpacedByAligned` keeps the gap between neighbours and places the group by its alignment
/// in the room left over, as Compose's `Arrangement.spacedBy(space, alignment)` does.
#[test]
fn spaced_rows_place_the_group_by_their_alignment() {
    let rects = rects();
    let probes = Rc::clone(&rects);
    let laid = run(Size::new(400.0, 300.0), &rects, move || {
        let probes = Rc::clone(&probes);
        row_arranged(
            Modifier::new().width(100.0),
            Arrangement::SpacedByAligned(10.0, Alignment::Center),
            Alignment::Start,
            move || {
                probe(&probes, "first", 20.0, 10.0);
                probe(&probes, "second", 20.0, 10.0);
            },
        );
    });
    assert_eq!(tuple(laid["first"]), (25.0, 0.0, 20.0, 10.0));
    assert_eq!(tuple(laid["second"]), (55.0, 0.0, 20.0, 10.0));
}

/// `SpacedBy` puts a fixed gap between neighbours.
#[test]
fn spaced_columns_keep_the_gap() {
    let rects = rects();
    let probes = Rc::clone(&rects);
    let laid = run(Size::new(400.0, 300.0), &rects, move || {
        let probes = Rc::clone(&probes);
        column_arranged(
            Modifier::new(),
            Arrangement::SpacedBy(8.0),
            Alignment::Start,
            move || {
                probe(&probes, "first", 10.0, 10.0);
                probe(&probes, "second", 10.0, 10.0);
            },
        );
    });
    assert_eq!(tuple(laid["second"]), (0.0, 18.0, 10.0, 10.0));
}

/// A child's own `align` wins over the row's alignment across the row.
#[test]
fn a_child_aligns_itself_across_a_row() {
    let rects = rects();
    let probes = Rc::clone(&rects);
    let laid = run(Size::new(400.0, 300.0), &rects, move || {
        let probes = Rc::clone(&probes);
        row_arranged(
            Modifier::new(),
            Arrangement::Start,
            Alignment::Start,
            move || {
                probe(&probes, "tall", 10.0, 40.0);
                probe_with(
                    &probes,
                    "low",
                    Modifier::new()
                        .size(10.0, 10.0)
                        .align(rustle_ui::BoxAlignment::BottomStart),
                );
            },
        );
    });
    assert_eq!(tuple(laid["low"]), (10.0, 30.0, 10.0, 10.0));
}

/// A child that matches its stack's size takes the size the other children gave the stack.
#[test]
fn a_child_can_match_its_stacks_size() {
    let rects = rects();
    let probes = Rc::clone(&rects);
    let laid = run(Size::new(400.0, 300.0), &rects, move || {
        let probes = Rc::clone(&probes);
        stack_with(Modifier::new(), move || {
            probe(&probes, "content", 60.0, 30.0);
            probe_with(&probes, "overlay", Modifier::new().match_parent_size());
        });
    });
    assert_eq!(tuple(laid["overlay"]), (0.0, 0.0, 60.0, 30.0));
}

/// Without a bound along the column, weighted children share only the column's least length,
/// none here, as in Compose.
#[test]
fn weights_in_an_unbounded_column_share_its_least_length() {
    let rects = rects();
    let probes = Rc::clone(&rects);
    let laid = run(Size::new(400.0, f32::INFINITY), &rects, move || {
        let probes = Rc::clone(&probes);
        column(move || {
            probe(&probes, "first", 10.0, 10.0);
            probe_with(&probes, "grown", Modifier::new().weight(1.0).width(10.0));
        });
    });
    assert_eq!(tuple(laid["grown"]), (0.0, 10.0, 10.0, 0.0));
}

/// A weighted child that does not fill takes at most its share, as long as its content wants.
#[test]
fn unfilled_weights_keep_their_own_length_within_their_share() {
    let rects = rects();
    let probes = Rc::clone(&rects);
    let laid = run(Size::new(400.0, 300.0), &rects, move || {
        let probes = Rc::clone(&probes);
        row_with(Modifier::new().width(100.0), move || {
            let inner = Rc::clone(&probes);
            row_with(Modifier::new().weight_with(1.0, false), move || {
                probe(&inner, "short", 30.0, 10.0)
            });
            let inner = Rc::clone(&probes);
            row_with(Modifier::new().weight_with(1.0, false), move || {
                probe(&inner, "long", 90.0, 10.0)
            });
            probe(&probes, "fixed", 20.0, 10.0);
        });
    });
    assert_eq!(tuple(laid["short"]), (0.0, 0.0, 30.0, 10.0));
    assert_eq!(laid["long"].origin.x, 30.0);
    assert_eq!(tuple(laid["fixed"]), (70.0, 0.0, 20.0, 10.0));
}

/// Without a bound along the column but with a least length, weighted children fill what the
/// least length leaves.
#[test]
fn weights_fill_a_columns_least_length() {
    let rects = rects();
    let probes = Rc::clone(&rects);
    let laid = run(Size::new(400.0, 300.0), &rects, move || {
        let probes = Rc::clone(&probes);
        rustle_ui::layout(Modifier::new(), ForcedHeight(100.0), move || {
            let probes = Rc::clone(&probes);
            column(move || {
                probe(&probes, "first", 10.0, 10.0);
                probe_with(&probes, "grown", Modifier::new().weight(1.0).width(10.0));
            })
        });
    });
    assert_eq!(tuple(laid["grown"]), (0.0, 10.0, 10.0, 90.0));
}

/// Measures its one child with no bound down its length and at least as tall as it holds.
struct ForcedHeight(f32);

impl rustle_ui::Layout for ForcedHeight {
    /// Measures the child at least this tall, unbounded downwards.
    fn measure(
        &self,
        children: &mut rustle_ui::Children,
        constraints: Constraints,
    ) -> rustle_ui::LayoutResult {
        let size = children.measure(
            0,
            Constraints {
                min_height: self.0,
                max_height: f32::INFINITY,
                ..constraints
            },
        );
        rustle_ui::LayoutResult {
            size,
            positions: vec![Offset::new(0.0, 0.0)],
        }
    }
}

/// A flow row centres each child across its line when asked to.
#[test]
fn flow_rows_align_children_across_their_line() {
    let rects = rects();
    let probes = Rc::clone(&rects);
    let laid = run(Size::new(400.0, 300.0), &rects, move || {
        let probes = Rc::clone(&probes);
        flow_row_with(
            Modifier::new(),
            FlowOptions::new(0.0, 0.0).item_alignment(Alignment::Center),
            move || {
                probe(&probes, "tall", 10.0, 40.0);
                probe(&probes, "short", 10.0, 10.0);
            },
        );
    });
    assert_eq!(tuple(laid["short"]), (10.0, 15.0, 10.0, 10.0));
}

/// Content that wraps its width without a bound keeps its own width and, aligned to the end,
/// runs out of its parent on the start side, as Compose's `wrapContentWidth(End, unbounded =
/// true)` places it.
#[test]
fn unbounded_wrapped_content_runs_past_its_parent() {
    let rects = rects();
    let probes = Rc::clone(&rects);
    let laid = run(Size::new(400.0, 300.0), &rects, move || {
        let probes = Rc::clone(&probes);
        stack_with(Modifier::new().size(50.0, 50.0), move || {
            probe_with(
                &probes,
                "wide",
                Modifier::new()
                    .wrap_content_width(rustle_ui::BoxAlignment::CenterEnd, true)
                    .size(80.0, 10.0),
            );
        });
    });
    assert_eq!(tuple(laid["wide"]), (-30.0, 0.0, 80.0, 10.0));
}

/// Content that wraps its height without a bound keeps its own height and, aligned to the top,
/// runs out of its parent at the bottom, as Compose's `wrapContentHeight(Top, unbounded = true)`
/// places it.
#[test]
fn unbounded_wrapped_columns_keep_their_childrens_heights() {
    let rects = rects();
    let probes = Rc::clone(&rects);
    let laid = run(Size::new(400.0, 300.0), &rects, move || {
        let probes = Rc::clone(&probes);
        column_with(
            Modifier::new()
                .height(30.0)
                .wrap_content_height(rustle_ui::BoxAlignment::TopStart, true),
            move || {
                probe(&probes, "first", 10.0, 20.0);
                probe(&probes, "second", 10.0, 20.0);
            },
        );
    });
    assert_eq!(tuple(laid["second"]), (0.0, 20.0, 10.0, 20.0));
}
