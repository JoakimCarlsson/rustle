//! Tests of animated content, animated size and shared elements over time, with the frame
//! clock advanced by hand and the rectangles read back from a real layout.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use rustle_animation::{AnimationSpec, Easing};
use rustle_foundation::{Modifier, canvas, column_with, row_with, stack_with};
use rustle_geometry::{Rect, Size};
use rustle_text::{LineLayout, StyledSpan, TextLayout, TextMeasurer, TextStyle};
use rustle_ui::{Composition, composition, display_list};
use rustle_unit::Constraints;

use super::*;

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

/// The rectangles probes recorded in the latest layout, by name.
type Found = Rc<RefCell<BTreeMap<u32, Rect>>>;

/// A composition laid out in a window whose clock the test moves.
struct Stage {
    composition: Composition,
    found: Found,
    now: Duration,
}

impl Stage {
    /// Composes `root` and settles layout.
    fn new(found: &Found, root: impl FnMut() + 'static) -> Self {
        let mut composition = composition(root);
        composition.compose();
        let mut stage = Self {
            composition,
            found: Rc::clone(found),
            now: Duration::ZERO,
        };
        stage.settle();
        stage
    }

    /// Lays out and recomposes until nothing changes, as the event loop does.
    fn settle(&mut self) {
        for _ in 0..30 {
            self.found.borrow_mut().clear();
            let tree = self.composition.output().cloned().expect("composed");
            let window = Constraints::loose(Size::new(400.0, 400.0));
            display_list(&tree, window, &mut NoText);
            if !self.composition.recompose() {
                return;
            }
        }
        panic!("layout does not settle");
    }

    /// Runs one frame `milliseconds` after the last and settles.
    fn frame(&mut self, milliseconds: u64) {
        self.now += Duration::from_millis(milliseconds);
        self.composition.run_frame(self.now);
        self.composition.recompose();
        self.settle();
    }

    /// Runs frames of 16 ms for `milliseconds`.
    fn run(&mut self, milliseconds: u64) {
        let end = self.now + Duration::from_millis(milliseconds);
        while self.now < end {
            self.frame(16.min((end - self.now).as_millis() as u64));
        }
    }

    /// Recomposes and settles after the test changed something the root reads.
    fn touch(&mut self) {
        self.composition.invalidate();
        self.composition.recompose();
        self.settle();
    }

    /// The rectangle of probe `name`, if it is in the latest layout.
    fn rect(&self, name: u32) -> Option<Rect> {
        self.found.borrow().get(&name).copied()
    }
}

/// Emits an empty area that records its rectangle under `name`.
fn probe(found: &Found, name: u32, modifier: Modifier) {
    let found = Rc::clone(found);
    canvas(
        modifier.on_globally_positioned(move |rect| {
            found.borrow_mut().insert(name, rect);
        }),
        |_| {},
    );
}

/// Asserts `a` is within a quarter pixel of `b`.
#[track_caller]
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.25, "{a} is not near {b}");
}

/// A linear tween of `milliseconds`.
fn linear(milliseconds: u64) -> AnimationSpec {
    AnimationSpec::Tween {
        duration: Duration::from_millis(milliseconds),
        easing: Easing::Linear,
    }
}

/// Content that shows the number `target` as a 40 by 20 probe, so each target has its own.
fn numbered(found: &Found, target: &Rc<RefCell<u32>>, transform: ContentTransform) -> Stage {
    let (found_in, target_in) = (Rc::clone(found), Rc::clone(target));
    Stage::new(found, move || {
        let found = Rc::clone(&found_in);
        animated_content(
            *target_in.borrow(),
            move |_, _| transform,
            move |value| probe(&found, *value, Modifier::new().size(40.0, 20.0)),
        );
    })
}

#[test]
fn content_slides_in_as_the_old_slides_out_and_is_removed_at_the_end() {
    let found = Found::default();
    let target = Rc::new(RefCell::new(1));
    let transform = ContentTransform::new(
        EnterTransition::slide_in_vertically(1.0),
        ExitTransition::slide_out_vertically(-1.0),
    )
    .with_spec(linear(100))
    .with_size_transform(false);
    let mut stage = numbered(&found, &target, transform);
    near(stage.rect(1).unwrap().origin.y, 0.0);
    *target.borrow_mut() = 2;
    stage.touch();
    near(stage.rect(2).unwrap().origin.y, 20.0);
    near(stage.rect(1).unwrap().origin.y, 0.0);
    stage.frame(16);
    stage.frame(50);
    near(stage.rect(2).unwrap().origin.y, 10.0);
    near(stage.rect(1).unwrap().origin.y, -10.0);
    stage.frame(30);
    near(stage.rect(2).unwrap().origin.y, 4.0);
    near(stage.rect(1).unwrap().origin.y, -16.0);
    stage.run(300);
    near(stage.rect(2).unwrap().origin.y, 0.0);
    assert!(stage.rect(1).is_none());
    assert!(!stage.composition.has_pending_frame());
}

#[test]
fn content_changed_midway_hands_over_cleanly() {
    let found = Found::default();
    let target = Rc::new(RefCell::new(1));
    let transform = ContentTransform::new(
        EnterTransition::slide_in_vertically(1.0),
        ExitTransition::slide_out_vertically(-1.0),
    )
    .with_spec(linear(100))
    .with_size_transform(false);
    let mut stage = numbered(&found, &target, transform);
    *target.borrow_mut() = 2;
    stage.touch();
    stage.frame(16);
    stage.frame(50);
    *target.borrow_mut() = 3;
    stage.touch();
    near(stage.rect(3).unwrap().origin.y, 20.0);
    assert!(stage.rect(2).is_some());
    stage.frame(16);
    stage.frame(50);
    near(stage.rect(3).unwrap().origin.y, 10.0);
    stage.run(300);
    near(stage.rect(3).unwrap().origin.y, 0.0);
    assert!(stage.rect(1).is_none() && stage.rect(2).is_none());
    assert!(!stage.composition.has_pending_frame());
}

#[test]
fn content_that_never_changes_asks_for_no_frames() {
    let found = Found::default();
    let target = Rc::new(RefCell::new(1));
    let mut stage = numbered(&found, &target, ContentTransform::default());
    stage.frame(16);
    stage.frame(16);
    assert!(!stage.composition.has_pending_frame());
}

#[test]
fn the_area_follows_the_size_of_the_content_as_it_changes() {
    let found = Found::default();
    let target = Rc::new(RefCell::new(1));
    let transform = ContentTransform::new(EnterTransition::none(), ExitTransition::none())
        .with_spec(linear(100));
    let (found_in, target_in) = (Rc::clone(&found), Rc::clone(&target));
    let mut stage = Stage::new(&found, move || {
        let found = Rc::clone(&found_in);
        column_with(Modifier::new(), || {
            animated_content(*target_in.borrow(), move |_, _| transform, {
                let found = Rc::clone(&found);
                move |value| {
                    let side = if *value == 1 { 40.0 } else { 100.0 };
                    probe(&found, *value, Modifier::new().size(side, side));
                }
            });
            probe(&found, 99, Modifier::new().size(10.0, 10.0));
        });
    });
    near(stage.rect(99).unwrap().origin.y, 40.0);
    *target.borrow_mut() = 2;
    stage.touch();
    near(stage.rect(99).unwrap().origin.y, 40.0);
    stage.frame(16);
    stage.frame(50);
    near(stage.rect(99).unwrap().origin.y, 70.0);
    stage.run(300);
    near(stage.rect(99).unwrap().origin.y, 100.0);
    *target.borrow_mut() = 1;
    stage.touch();
    stage.frame(16);
    stage.frame(50);
    near(stage.rect(99).unwrap().origin.y, 70.0);
    stage.run(300);
    near(stage.rect(99).unwrap().origin.y, 40.0);
}

#[test]
fn animate_content_size_glides_each_way_and_settles() {
    let found = Found::default();
    let side = Rc::new(RefCell::new(20.0_f32));
    let (found_in, side_in) = (Rc::clone(&found), Rc::clone(&side));
    let mut stage = Stage::new(&found, move || {
        let found = Rc::clone(&found_in);
        let side = *side_in.borrow();
        row_with(Modifier::new(), || {
            animate_content_size(Modifier::new(), linear(100), || {
                probe(&found, 1, Modifier::new().size(side, side));
            });
            probe(&found, 2, Modifier::new().size(5.0, 5.0));
        });
    });
    near(stage.rect(2).unwrap().origin.x, 20.0);
    *side.borrow_mut() = 120.0;
    stage.touch();
    near(stage.rect(2).unwrap().origin.x, 20.0);
    stage.frame(16);
    stage.frame(25);
    near(stage.rect(2).unwrap().origin.x, 45.0);
    stage.frame(50);
    near(stage.rect(2).unwrap().origin.x, 95.0);
    stage.run(300);
    near(stage.rect(2).unwrap().origin.x, 120.0);
    assert!(!stage.composition.has_pending_frame());
    *side.borrow_mut() = 20.0;
    stage.touch();
    stage.frame(16);
    stage.frame(50);
    near(stage.rect(2).unwrap().origin.x, 70.0);
    stage.run(300);
    near(stage.rect(2).unwrap().origin.x, 20.0);
}

#[test]
fn a_shared_element_glides_from_where_its_twin_was() {
    let found = Found::default();
    let left = Rc::new(RefCell::new(true));
    let (found_in, left_in) = (Rc::clone(&found), Rc::clone(&left));
    let mut stage = Stage::new(&found, move || {
        let elements = remember_shared_elements();
        let found = Rc::clone(&found_in);
        stack_with(Modifier::new().size(300.0, 100.0), || {
            if *left_in.borrow() {
                shared_element(
                    &elements,
                    "badge",
                    Modifier::new().offset(10.0, 0.0),
                    linear(100),
                    || {
                        probe(&found, 1, Modifier::new().size(20.0, 20.0));
                    },
                );
            } else {
                shared_element(
                    &elements,
                    "badge",
                    Modifier::new().offset(200.0, 0.0),
                    linear(100),
                    || {
                        probe(&found, 2, Modifier::new().size(20.0, 20.0));
                    },
                );
            }
        });
    });
    near(stage.rect(1).unwrap().origin.x, 10.0);
    *left.borrow_mut() = false;
    stage.touch();
    stage.frame(16);
    stage.frame(0);
    near(stage.rect(2).unwrap().origin.x, 10.0);
    stage.frame(50);
    near(stage.rect(2).unwrap().origin.x, 105.0);
    stage.run(300);
    near(stage.rect(2).unwrap().origin.x, 200.0);
    assert!(!stage.composition.has_pending_frame());
}
