//! Tests of painting that reuses what the list painted before holds of unchanged subtrees:
//! a clean subtree is neither laid out nor painted again, and the list is the one painting
//! everything makes.

use std::cell::{Cell, RefCell};

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Color, StyledSpan};
use rustle_runtime::{State, remember};
use rustle_text::{LineLayout, TextLayout, TextMeasurer, TextStyle};
use rustle_unit::Constraints;

use super::*;

/// A measurer giving every character eight pixels on a sixteen pixel line.
struct Fixed;

impl TextMeasurer for Fixed {
    /// Eight pixels a character, one line tall.
    fn measure(&mut self, content: &str, _: &TextStyle, _: f32) -> Size {
        Size::new(content.chars().count() as f32 * 8.0, 16.0)
    }

    /// Measures the joined text of `spans`.
    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, width: f32) -> Size {
        let content: String = spans.iter().map(|span| span.text.as_str()).collect();
        self.measure(&content, style, width)
    }

    /// One empty line; nothing here reads it.
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

/// A column of children placed one under the other, counting how often it is measured.
struct Column {
    /// How many times it was measured.
    measured: Rc<Cell<usize>>,
}

impl Layout for Column {
    /// Stacks the children top to bottom at their own sizes, counting the call.
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        self.measured.set(self.measured.get() + 1);
        let loose = Constraints {
            min_width: 0.0,
            min_height: 0.0,
            max_height: f32::INFINITY,
            ..constraints
        };
        let mut positions = Vec::new();
        let mut size = Size::default();
        for index in 0..children.len() {
            let child = children.measure(index, loose);
            positions.push(Offset::new(0.0, size.height));
            size.width = size.width.max(child.width);
            size.height += child.height;
        }
        LayoutResult { size, positions }
    }
}

/// A column that counts nothing.
fn column() -> Column {
    Column {
        measured: Rc::new(Cell::new(0)),
    }
}

/// Emits a text node.
fn label(value: impl ToString) {
    emit(Node {
        modifier: Modifier::new(),
        content: Content::Text {
            content: value.to_string(),
            style: TextStyle::default(),
            editing: None,
        },
    });
}

/// Adds one to `counter`.
fn bump(counter: &Cell<usize>) {
    counter.set(counter.get() + 1);
}

/// Everything in `list` that painting decides, written out to compare two lists.
fn fingerprint(list: &DisplayList) -> String {
    let mut out = format!("commands {:?}\n", list.commands);
    for region in &list.regions {
        out += &format!("hit {:?} {:?}\n", region.rect, region.buttons_inside);
    }
    for region in &list.buttons {
        out += &format!("button {:?} {:?}\n", region.rect, region.origin);
    }
    for region in &list.focusables {
        out += &format!(
            "focusable {:?} {:?} {:?} {}\n",
            region.rect,
            region.bounds,
            region.scrolls,
            region.focus.id()
        );
    }
    for node in &list.semantics {
        out += &format!(
            "semantics {:?} {:?} {:?} {:?}\n",
            node.rect,
            node.parent,
            node.semantics.label,
            node.on_click.is_some()
        );
    }
    for tag in &list.test_tags {
        out += &format!("tag {tag:?}\n");
    }
    for region in &list.interactions {
        out += &format!(
            "interaction {:?} {:?} {} {:?}\n",
            region.rect, region.group, region.covered_after, region.interaction
        );
    }
    for region in &list.hovers {
        out += &format!("hover {:?} {}\n", region.rect, region.covered_after);
    }
    for region in &list.scrolls {
        out += &format!("scroll {:?}\n", region.rect);
    }
    for region in &list.drags {
        out += &format!("drag {:?} {}\n", region.rect, region.inputs_before);
    }
    for region in &list.presses {
        out += &format!("press {:?} {:?}\n", region.rect, region.origin);
    }
    for region in &list.key_handlers {
        out += &format!("keys {:?} {:?}\n", region.rect, region.focusables);
    }
    out += &format!("ime {:?}\n", list.ime_area);
    out
}

/// Lays `composition`'s tree out in 200 by 300.
fn lay_out(composition: &Composition) -> LaidOut {
    LaidOut::new(
        composition.output().unwrap(),
        Constraints::loose(Size::new(200.0, 300.0)),
        &mut Fixed,
        DisplayOptions::default(),
    )
}

/// What a test sees of the blocks of [`two_blocks`].
struct Blocks {
    /// The number of lines of the first block, which sets its height.
    lines: State<usize>,
    /// How often the first block's canvas was drawn.
    first_drawn: Rc<Cell<usize>>,
    /// How often the second block's canvas was drawn.
    second_drawn: Rc<Cell<usize>>,
    /// How often the second block's layout was measured.
    second_measured: Rc<Cell<usize>>,
    /// Every place the second block was told it was placed at.
    second_placed: Rc<RefCell<Vec<Rect>>>,
}

/// A composition of two blocks in a column: a canvas as tall as ten pixels a line, and under
/// it a clickable, described, tagged block holding a canvas.
fn two_blocks() -> (Composition, Rc<RefCell<Option<Blocks>>>) {
    let seen = Rc::new(RefCell::new(None::<Blocks>));
    let kept = Rc::clone(&seen);
    let first_drawn = Rc::new(Cell::new(0));
    let second_drawn = Rc::new(Cell::new(0));
    let second_measured = Rc::new(Cell::new(0));
    let second_placed = Rc::new(RefCell::new(Vec::new()));
    let composition = composition(move || {
        let lines = remember(|| 1usize);
        *kept.borrow_mut() = Some(Blocks {
            lines: lines.clone(),
            first_drawn: Rc::clone(&first_drawn),
            second_drawn: Rc::clone(&second_drawn),
            second_measured: Rc::clone(&second_measured),
            second_placed: Rc::clone(&second_placed),
        });
        let (first_drawn, second_drawn) = (Rc::clone(&first_drawn), Rc::clone(&second_drawn));
        let (second_measured, second_placed) =
            (Rc::clone(&second_measured), Rc::clone(&second_placed));
        layout(Modifier::new(), column(), move || {
            let (lines, first_drawn) = (lines.clone(), Rc::clone(&first_drawn));
            restartable((), move || {
                let first_drawn = Rc::clone(&first_drawn);
                let height = 10.0 * lines.get() as f32;
                canvas(Modifier::new().size(100.0, height), move |canvas| {
                    bump(&first_drawn);
                    canvas.rect(0.0, 0.0, 100.0, height, Color::rgba(200, 0, 0, 255));
                });
            });
            let (second_drawn, second_measured) =
                (Rc::clone(&second_drawn), Rc::clone(&second_measured));
            let second_placed = Rc::clone(&second_placed);
            restartable((), move || {
                let second_drawn = Rc::clone(&second_drawn);
                let placed = Rc::clone(&second_placed);
                let modifier = Modifier::new()
                    .test_tag("second")
                    .semantics(crate::semantics::Semantics::new(
                        crate::semantics::Role::Button,
                    ))
                    .on_placed(move |rect, _| placed.borrow_mut().push(rect))
                    .clickable(|| {});
                let measured = Column {
                    measured: Rc::clone(&second_measured),
                };
                layout(modifier, measured, move || {
                    let second_drawn = Rc::clone(&second_drawn);
                    canvas(Modifier::new().size(50.0, 20.0), move |canvas| {
                        bump(&second_drawn);
                        canvas.rect(0.0, 0.0, 50.0, 20.0, Color::rgba(0, 0, 200, 255));
                    });
                    label("second");
                });
            });
        });
    });
    (composition, seen)
}

/// A change to one block lays out and paints only that block: the other is reused where it
/// was, its layout not measured and its canvas not drawn, and the list is what painting
/// everything makes.
#[test]
fn a_clean_subtree_is_neither_laid_out_nor_painted_again() {
    let (mut composition, seen) = two_blocks();
    composition.compose();
    let pointer = PointerState::default();
    let first = lay_out(&composition).paint_reusing(pointer, DisplayList::default());
    let blocks = seen.borrow_mut().take().unwrap();
    assert_eq!(blocks.first_drawn.get(), 1);
    assert_eq!(blocks.second_drawn.get(), 1);
    assert_eq!(blocks.second_measured.get(), 1);
    assert_eq!(first.reused_subtrees(), 0);

    let laid_out = lay_out(&composition);
    let again = laid_out.paint_reusing(pointer, first);
    assert_eq!(blocks.first_drawn.get(), 1);
    assert_eq!(blocks.second_drawn.get(), 1);
    assert_eq!(blocks.second_measured.get(), 1);
    assert!(again.reused_subtrees() >= 1);
    assert_eq!(fingerprint(&again), fingerprint(&laid_out.paint(pointer)));

    blocks.lines.set(2);
    assert!(composition.recompose());
    let drawn = (blocks.first_drawn.get(), blocks.second_drawn.get());
    let laid_out = lay_out(&composition);
    let changed = laid_out.paint_reusing(pointer, again);
    assert_eq!(blocks.first_drawn.get(), drawn.0 + 1);
    assert_eq!(blocks.second_drawn.get(), drawn.1);
    assert_eq!(blocks.second_measured.get(), 1);
    assert_eq!(changed.reused_subtrees(), 1);
    assert_eq!(fingerprint(&changed), fingerprint(&laid_out.paint(pointer)));
}

/// A clean block pushed down by its sibling is reused moved to its new place: its drawing,
/// areas and description move with it, and its placement handler is told the new place.
#[test]
fn a_moved_subtree_is_reused_where_it_is_now() {
    let (mut composition, seen) = two_blocks();
    composition.compose();
    let pointer = PointerState::default();
    let first = lay_out(&composition).paint_reusing(pointer, DisplayList::default());
    let blocks = seen.borrow_mut().take().unwrap();
    assert_eq!(blocks.second_placed.borrow().last().unwrap().origin.y, 10.0);

    blocks.lines.set(4);
    assert!(composition.recompose());
    let drawn = blocks.second_drawn.get();
    let laid_out = lay_out(&composition);
    let moved = laid_out.paint_reusing(pointer, first);
    assert_eq!(blocks.second_drawn.get(), drawn);
    assert_eq!(blocks.second_measured.get(), 1);
    assert_eq!(moved.reused_subtrees(), 1);
    assert_eq!(blocks.second_placed.borrow().last().unwrap().origin.y, 40.0);
    assert_eq!(moved.test_tags[0].bounds.origin.y, 40.0);
    assert_eq!(moved.regions[0].rect.origin.y, 40.0);
    assert_eq!(fingerprint(&moved), fingerprint(&laid_out.paint(pointer)));
}

/// State read only while drawing, such as a spinner's angle, paints its own block again and
/// reuses the rest, without laying anything out.
#[test]
fn state_read_while_drawing_repaints_only_its_subtree() {
    let angle = Rc::new(RefCell::new(None::<State<f32>>));
    let (spinner_drawn, other_drawn) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (kept, spinner, other) = (
        Rc::clone(&angle),
        Rc::clone(&spinner_drawn),
        Rc::clone(&other_drawn),
    );
    let mut composition = composition(move || {
        let turn = remember(|| 0.0f32);
        *kept.borrow_mut() = Some(turn.clone());
        let (spinner, other) = (Rc::clone(&spinner), Rc::clone(&other));
        layout(Modifier::new(), column(), move || {
            let (spinner, turn) = (Rc::clone(&spinner), turn.clone());
            restartable((), move || {
                let (spinner, turn) = (Rc::clone(&spinner), turn.clone());
                canvas(Modifier::new().size(20.0, 20.0), move |canvas| {
                    bump(&spinner);
                    canvas.rect(turn.get(), 0.0, 2.0, 20.0, Color::rgba(0, 0, 0, 255));
                });
            });
            let other = Rc::clone(&other);
            restartable((), move || {
                let other = Rc::clone(&other);
                layout(Modifier::new(), column(), move || {
                    let other = Rc::clone(&other);
                    canvas(Modifier::new().size(80.0, 20.0), move |canvas| {
                        bump(&other);
                        canvas.rect(0.0, 0.0, 80.0, 20.0, Color::rgba(0, 90, 0, 255));
                    });
                    label("still");
                });
            });
        });
    });
    composition.compose();
    let pointer = PointerState::default();
    let laid_out = lay_out(&composition);
    let first = laid_out.paint_reusing(pointer, DisplayList::default());
    angle.borrow().clone().unwrap().set(7.0);
    let repainted = laid_out.paint_reusing(pointer, first);
    assert_eq!(spinner_drawn.get(), 2);
    assert_eq!(other_drawn.get(), 1);
    assert_eq!(repainted.reused_subtrees(), 1);
    assert_eq!(
        fingerprint(&repainted),
        fingerprint(&laid_out.paint(pointer))
    );
}

/// Moving the pointer onto an area drawn by its interaction state paints that area's block
/// again and reuses the one beside it.
#[test]
fn an_interaction_change_repaints_only_its_subtree() {
    let (hover_drawn, other_drawn) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (hover, other) = (Rc::clone(&hover_drawn), Rc::clone(&other_drawn));
    let mut composition = composition(move || {
        let (hover, other) = (Rc::clone(&hover), Rc::clone(&other));
        layout(Modifier::new(), column(), move || {
            let hover = Rc::clone(&hover);
            restartable((), move || {
                let hover = Rc::clone(&hover);
                let modifier = Modifier::new().size(60.0, 20.0).draw_behind_interactive(
                    move |canvas, interaction| {
                        bump(&hover);
                        let shade = if interaction.hovered { 255 } else { 40 };
                        canvas.rect(0.0, 0.0, 60.0, 20.0, Color::rgba(shade, 0, 0, 255));
                    },
                );
                layout(modifier, column(), || label("hover"));
            });
            let other = Rc::clone(&other);
            restartable((), move || {
                let other = Rc::clone(&other);
                canvas(Modifier::new().size(80.0, 20.0), move |canvas| {
                    bump(&other);
                    canvas.rect(0.0, 0.0, 80.0, 20.0, Color::rgba(0, 0, 90, 255));
                });
            });
        });
    });
    composition.compose();
    let laid_out = lay_out(&composition);
    let first = laid_out.paint_reusing(PointerState::default(), DisplayList::default());
    let over = PointerState {
        position: Some(Offset::new(10.0, 10.0)),
        pressed_at: None,
    };
    assert!(first.interactions_changed(over));
    let (hovered, others) = (hover_drawn.get(), other_drawn.get());
    let repainted = laid_out.paint_reusing(over, first);
    assert!(hover_drawn.get() > hovered);
    assert_eq!(other_drawn.get(), others);
    assert!(repainted.interactions[0].interaction.hovered);
    assert_eq!(fingerprint(&repainted), fingerprint(&laid_out.paint(over)));
}

/// Scrolling moves the rows inside the scrolling area without painting them again, and the
/// list is what painting everything makes; rows the scroll cuts are painted again.
#[test]
fn scrolled_rows_are_reused_where_they_are_now() {
    let scroll = Rc::new(RefCell::new(None::<ScrollState>));
    let drawn = Rc::new(Cell::new(0));
    let (kept, counted) = (Rc::clone(&scroll), Rc::clone(&drawn));
    let mut composition = composition(move || {
        let state = remember_scroll_state();
        *kept.borrow_mut() = Some(state.clone());
        let counted = Rc::clone(&counted);
        restartable((), move || {
            let counted = Rc::clone(&counted);
            layout(
                Modifier::new().size(100.0, 64.0).vertical_scroll(&state),
                column(),
                move || {
                    for row in 0..8 {
                        let counted = Rc::clone(&counted);
                        restartable(row, move || {
                            let counted = Rc::clone(&counted);
                            layout(Modifier::new().clickable(|| {}), column(), move || {
                                let counted = Rc::clone(&counted);
                                canvas(Modifier::new().size(90.0, 16.0), move |canvas| {
                                    bump(&counted);
                                    canvas.rect(0.0, 0.0, 90.0, 16.0, Color::rgba(9, 9, 9, 255));
                                });
                            });
                        });
                    }
                },
            );
        });
    });
    composition.compose();
    let pointer = PointerState::default();
    let first = lay_out(&composition).paint_reusing(pointer, DisplayList::default());
    let first = lay_out(&composition).paint_reusing(pointer, first);
    let state = scroll.borrow().clone().unwrap();
    state.scroll_to(8.0);
    composition.recompose();
    let before = drawn.get();
    let laid_out = lay_out(&composition);
    let scrolled = laid_out.paint_reusing(pointer, first);
    assert!(scrolled.reused_subtrees() >= 2);
    assert!(drawn.get() - before < 8);
    assert_eq!(
        fingerprint(&scrolled),
        fingerprint(&laid_out.paint(pointer))
    );
}

/// A list painted without the one before it reuses nothing and paints every subtree.
#[test]
fn painting_afresh_reuses_nothing() {
    let (mut composition, _) = two_blocks();
    composition.compose();
    let laid_out = lay_out(&composition);
    let first = laid_out.paint(PointerState::default());
    assert_eq!(first.reused_subtrees(), 0);
    assert!(first.repainted_subtrees() >= 2);
    let again = laid_out.paint(PointerState::default());
    assert_eq!(again.reused_subtrees(), 0);
}

/// With edges rounded to the pixel grid, a block moved by whole pixels is reused, and the list
/// is what painting everything makes.
#[test]
fn a_subtree_moved_on_the_pixel_grid_is_reused() {
    let (mut composition, seen) = two_blocks();
    composition.compose();
    let options = DisplayOptions {
        rounding: PixelRounding::Physical,
        scale: 2.0,
    };
    let lay_out = |composition: &Composition| {
        LaidOut::new(
            composition.output().unwrap(),
            Constraints::loose(Size::new(200.0, 300.0)),
            &mut Fixed,
            options,
        )
    };
    let pointer = PointerState::default();
    let first = lay_out(&composition).paint_reusing(pointer, DisplayList::default());
    let blocks = seen.borrow_mut().take().unwrap();
    blocks.lines.set(3);
    assert!(composition.recompose());
    let drawn = blocks.second_drawn.get();
    let laid_out = lay_out(&composition);
    let moved = laid_out.paint_reusing(pointer, first);
    assert_eq!(blocks.second_drawn.get(), drawn);
    assert_eq!(moved.reused_subtrees(), 1);
    assert_eq!(fingerprint(&moved), fingerprint(&laid_out.paint(pointer)));
}

/// A block whose text tells its layout keeps its layout, and the handler is told the layout
/// again each time the block is reused.
#[test]
fn a_block_telling_its_text_layout_keeps_its_layout() {
    let measured = Rc::new(Cell::new(0));
    let told = Rc::new(Cell::new(0));
    let (counted, heard) = (Rc::clone(&measured), Rc::clone(&told));
    let mut composition = composition(move || {
        let (counted, heard) = (Rc::clone(&counted), Rc::clone(&heard));
        restartable((), move || {
            let (counted, heard) = (Rc::clone(&counted), Rc::clone(&heard));
            layout(Modifier::new(), Column { measured: counted }, move || {
                let heard = Rc::clone(&heard);
                emit(Node {
                    modifier: Modifier::new().on_text_layout(move |_| bump(&heard)),
                    content: Content::Text {
                        content: "told".to_string(),
                        style: TextStyle::default(),
                        editing: None,
                    },
                });
            });
        });
    });
    composition.compose();
    lay_out(&composition);
    assert_eq!((measured.get(), told.get()), (1, 1));
    lay_out(&composition);
    assert_eq!((measured.get(), told.get()), (1, 2));
}

/// A block inside an area with a drag-end handler composed afresh each frame is still reused,
/// since nothing in it takes the handler.
#[test]
fn a_fresh_drag_end_handler_around_a_block_leaves_it_reused() {
    let drawn = Rc::new(Cell::new(0));
    let counted = Rc::clone(&drawn);
    let mut composition = composition(move || {
        let counted = Rc::clone(&counted);
        layout(Modifier::new().on_drag_end(|| {}), column(), move || {
            let counted = Rc::clone(&counted);
            restartable((), move || {
                let counted = Rc::clone(&counted);
                canvas(Modifier::new().size(40.0, 20.0), move |canvas| {
                    bump(&counted);
                    canvas.rect(0.0, 0.0, 40.0, 20.0, Color::rgba(1, 2, 3, 255));
                });
            });
        });
    });
    composition.compose();
    let pointer = PointerState::default();
    let first = lay_out(&composition).paint_reusing(pointer, DisplayList::default());
    composition.invalidate();
    assert!(composition.recompose());
    let laid_out = lay_out(&composition);
    let again = laid_out.paint_reusing(pointer, first);
    assert_eq!(drawn.get(), 1);
    assert_eq!(again.reused_subtrees(), 1);
    assert_eq!(fingerprint(&again), fingerprint(&laid_out.paint(pointer)));
}
