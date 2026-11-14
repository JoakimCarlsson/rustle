//! Tests of restartable blocks: scoped recomposition and the layout cache that follows it.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use rustle_geometry::{Offset, Size};
use rustle_graphics::{DrawCommand, StyledSpan};
use rustle_runtime::{State, remember};
use rustle_text::{LineLayout, TextLayout, TextMeasurer, TextStyle};
use rustle_unit::Constraints;

use super::*;

/// A fixed-width measurer that counts how often each string was measured.
#[derive(Default)]
struct Counting {
    measured: HashMap<String, usize>,
}

impl TextMeasurer for Counting {
    /// Returns eight pixels per character and counts the call.
    fn measure(&mut self, content: &str, _: &TextStyle, _: f32) -> Size {
        *self.measured.entry(content.to_string()).or_default() += 1;
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

/// The strings of the text commands in `list`.
fn texts(list: &DisplayList) -> Vec<String> {
    list.commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect()
}

/// Two blocks each read their own state; changing one recomposes and measures only that one.
#[test]
fn a_change_recomposes_and_measures_only_the_block_that_read_it() {
    let states = Rc::new(RefCell::new(Vec::<State<i32>>::new()));
    let runs = Rc::new(Cell::new((0, 0, 0)));
    let (kept, counted) = (Rc::clone(&states), Rc::clone(&runs));
    let mut composition = composition(move || {
        let (first, second) = (remember(|| 0), remember(|| 0));
        *kept.borrow_mut() = vec![first.clone(), second.clone()];
        let seen = Rc::clone(&counted);
        counted.set((counted.get().0 + 1, counted.get().1, counted.get().2));
        layout(Modifier::new(), Overlay::default(), move || {
            let (first_runs, second_runs) = (Rc::clone(&seen), Rc::clone(&seen));
            let first = first.clone();
            restartable((), move || {
                first_runs.set((
                    first_runs.get().0,
                    first_runs.get().1 + 1,
                    first_runs.get().2,
                ));
                label(format!("first {}", first.get()));
            });
            let second = second.clone();
            restartable((), move || {
                second_runs.set((
                    second_runs.get().0,
                    second_runs.get().1,
                    second_runs.get().2 + 1,
                ));
                label(format!("second {}", second.get()));
            });
        });
    });
    composition.compose();
    let mut text = Counting::default();
    let constraints = Constraints::loose(Size::new(200.0, 100.0));
    let list = display_list(composition.output().unwrap(), constraints, &mut text);
    assert_eq!(texts(&list), ["first 0", "second 0"]);
    assert_eq!(text.measured["second 0"], 1);

    let unchanged = display_list(composition.output().unwrap(), constraints, &mut text);
    assert_eq!(texts(&unchanged), ["first 0", "second 0"]);
    assert_eq!(text.measured["second 0"], 1);

    states.borrow()[0].set(1);
    assert!(composition.recompose());
    assert_eq!(runs.get(), (1, 2, 1));
    let list = display_list(composition.output().unwrap(), constraints, &mut text);
    assert_eq!(texts(&list), ["first 1", "second 0"]);
    assert_eq!(text.measured["first 1"], 1);
    assert_eq!(text.measured["second 0"], 1);
}

/// A cached layout is measured again when the constraints change.
#[test]
fn a_cached_layout_follows_the_constraints() {
    let mut composition = composition(|| {
        restartable((), || label("fixed"));
    });
    composition.compose();
    let mut text = Counting::default();
    let wide = Constraints::loose(Size::new(200.0, 100.0));
    let narrow = Constraints::loose(Size::new(20.0, 100.0));
    display_list(composition.output().unwrap(), wide, &mut text);
    display_list(composition.output().unwrap(), wide, &mut text);
    assert_eq!(text.measured["fixed"], 1);
    display_list(composition.output().unwrap(), narrow, &mut text);
    assert_eq!(text.measured["fixed"], 2);
}

/// State read while measuring, such as a scroll offset, invalidates the cached layout.
#[test]
fn state_read_while_measuring_invalidates_the_cached_layout() {
    let states = Rc::new(RefCell::new(None::<State<i32>>));
    let kept = Rc::clone(&states);
    let mut composition = composition(move || {
        *kept.borrow_mut() = Some(remember(|| 0));
        let offset = kept.borrow().clone().unwrap();
        restartable((), move || {
            let offset = offset.clone();
            emit(Node {
                modifier: Modifier::new().layout(move |measurable, constraints| {
                    let _ = offset.get();
                    let size = measurable.measure(constraints);
                    Placement::new(size, rustle_geometry::Offset::default())
                }),
                content: Content::Text {
                    content: "probe".to_string(),
                    style: TextStyle::default(),
                    editing: None,
                },
            });
        });
    });
    composition.compose();
    let mut text = Counting::default();
    let constraints = Constraints::loose(Size::new(200.0, 100.0));
    display_list(composition.output().unwrap(), constraints, &mut text);
    display_list(composition.output().unwrap(), constraints, &mut text);
    assert_eq!(text.measured["probe"], 1);
    states.borrow().clone().unwrap().set(1);
    composition.recompose();
    display_list(composition.output().unwrap(), constraints, &mut text);
    assert_eq!(text.measured["probe"], 2);
}

/// A column of children placed one under the other, left-aligned.
struct Vertical;

impl Layout for Vertical {
    /// Stacks the children top to bottom at their own widths.
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
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

/// Where each text command of `list` was drawn, by content.
fn places(list: &DisplayList) -> Vec<(String, f32, f32)> {
    list.commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { content, x, y, .. } => Some((content.clone(), *x, *y)),
            _ => None,
        })
        .collect()
}

/// Lays out `composition` loosely in 200 by 300 with `text`.
fn lay_out(composition: &Composition, text: &mut Counting) -> DisplayList {
    display_list(
        composition.output().unwrap(),
        Constraints::loose(Size::new(200.0, 300.0)),
        text,
    )
}

/// A scope inside a scope changes size: the outer layout is measured again around it while the
/// sibling's measurement is reused, and everything lands where it should.
#[test]
fn an_inner_change_relayouts_the_outer_around_it() {
    let state = Rc::new(RefCell::new(None::<State<usize>>));
    let kept = Rc::clone(&state);
    let mut composition = composition(move || {
        let lines = remember(|| 1);
        *kept.borrow_mut() = Some(lines.clone());
        restartable((), move || {
            let lines = lines.clone();
            layout(Modifier::new(), Vertical, move || {
                label("head");
                let lines = lines.clone();
                restartable((), move || {
                    layout(Modifier::new(), Vertical, || {
                        for line in 0..lines.get() {
                            label(format!("line {line}"));
                        }
                    });
                });
                label("tail");
            });
        });
    });
    composition.compose();
    let mut text = Counting::default();
    let first = lay_out(&composition, &mut text);
    assert_eq!(
        places(&first),
        [
            ("head".to_string(), 0.0, 0.0),
            ("line 0".to_string(), 0.0, 16.0),
            ("tail".to_string(), 0.0, 32.0)
        ]
    );
    state.borrow().clone().unwrap().set(3);
    composition.recompose();
    let second = lay_out(&composition, &mut text);
    assert_eq!(
        places(&second),
        [
            ("head".to_string(), 0.0, 0.0),
            ("line 0".to_string(), 0.0, 16.0),
            ("line 1".to_string(), 0.0, 32.0),
            ("line 2".to_string(), 0.0, 48.0),
            ("tail".to_string(), 0.0, 64.0)
        ]
    );
    assert_eq!(text.measured["line 0"], 2);
    assert_eq!(text.measured["line 2"], 1);
    let third = lay_out(&composition, &mut text);
    assert_eq!(places(&third), places(&second));
    assert_eq!(text.measured["line 0"], 2);
    assert_eq!(text.measured["tail"], 2);
}

/// Keyed items inside a block follow a reorder and an insertion with their layout.
#[test]
fn keyed_items_in_a_block_follow_the_list() {
    let state = Rc::new(RefCell::new(None::<State<Vec<u32>>>));
    let kept = Rc::clone(&state);
    let mut composition = composition(move || {
        let items = remember(|| vec![1, 2, 3]);
        *kept.borrow_mut() = Some(items.clone());
        restartable((), move || {
            let items = items.clone();
            layout(Modifier::new(), Vertical, move || {
                for id in items.get() {
                    rustle_runtime::key(id, || {
                        restartable(id, move || label(format!("item {id}")));
                    });
                }
            });
        });
    });
    composition.compose();
    let mut text = Counting::default();
    lay_out(&composition, &mut text);
    state.borrow().clone().unwrap().set(vec![3, 4, 1]);
    composition.recompose();
    let list = lay_out(&composition, &mut text);
    assert_eq!(
        places(&list),
        [
            ("item 3".to_string(), 0.0, 0.0),
            ("item 4".to_string(), 0.0, 16.0),
            ("item 1".to_string(), 0.0, 32.0)
        ]
    );
    assert_eq!(text.measured["item 1"], 1);
    assert_eq!(text.measured["item 3"], 1);
    assert_eq!(text.measured["item 4"], 1);
}

/// A conditional child appears and disappears with its state, and the size follows.
#[test]
fn conditionals_in_a_block_change_the_size() {
    let state = Rc::new(RefCell::new(None::<State<bool>>));
    let kept = Rc::clone(&state);
    let mut composition = composition(move || {
        let show = remember(|| false);
        *kept.borrow_mut() = Some(show.clone());
        restartable((), move || {
            let show = show.clone();
            layout(Modifier::new(), Vertical, move || {
                label("always");
                if show.get() {
                    label("sometimes");
                }
            });
        });
    });
    composition.compose();
    let mut text = Counting::default();
    let names = |composition: &Composition, text: &mut Counting| {
        places(&lay_out(composition, text))
            .into_iter()
            .map(|(name, _, y)| (name, y))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&composition, &mut text),
        [("always".to_string(), 0.0)]
    );
    state.borrow().clone().unwrap().set(true);
    composition.recompose();
    assert_eq!(
        names(&composition, &mut text),
        [("always".to_string(), 0.0), ("sometimes".to_string(), 16.0)]
    );
    state.borrow().clone().unwrap().set(false);
    composition.recompose();
    assert_eq!(
        names(&composition, &mut text),
        [("always".to_string(), 0.0)]
    );
}

/// Moving a scroll offset moves the content even though the block's layout is cached.
#[test]
fn scrolling_invalidates_the_cached_layout() {
    let scroll = Rc::new(RefCell::new(None::<ScrollState>));
    let kept = Rc::clone(&scroll);
    let mut composition = composition(move || {
        let state = remember_scroll_state();
        *kept.borrow_mut() = Some(state.clone());
        restartable((), move || {
            let state = state.clone();
            layout(
                Modifier::new().size(100.0, 40.0).vertical_scroll(&state),
                Vertical,
                || {
                    for line in 0..5 {
                        label(format!("row {line}"));
                    }
                },
            );
        });
    });
    composition.compose();
    let mut text = Counting::default();
    lay_out(&composition, &mut text);
    let list = lay_out(&composition, &mut text);
    assert_eq!(places(&list)[1], ("row 1".to_string(), 0.0, 16.0));
    let state = scroll.borrow().clone().unwrap();
    assert_eq!(state.max_offset(), 40.0);
    state.scroll_to(24.0);
    composition.recompose();
    let list = lay_out(&composition, &mut text);
    assert_eq!(places(&list)[0], ("row 0".to_string(), 0.0, -24.0));
    assert_eq!(places(&list)[2], ("row 2".to_string(), 0.0, 8.0));
}

/// The layout direction read inside a block follows the local provided around it.
#[test]
fn locals_read_in_a_block_follow_their_provider() {
    let state = Rc::new(RefCell::new(None::<State<LayoutDirection>>));
    let kept = Rc::clone(&state);
    let mut composition = composition(move || {
        let direction = remember(|| LayoutDirection::Ltr);
        *kept.borrow_mut() = Some(direction.clone());
        LOCAL_LAYOUT_DIRECTION.provide(direction.get(), || {
            restartable((), || {
                layout(
                    Modifier::new().size(100.0, 20.0),
                    Overlay {
                        rtl: is_rtl(),
                        ..Overlay::default()
                    },
                    || label("abcd"),
                );
            });
        });
    });
    composition.compose();
    let mut text = Counting::default();
    assert_eq!(places(&lay_out(&composition, &mut text))[0].1, 0.0);
    state.borrow().clone().unwrap().set(LayoutDirection::Rtl);
    composition.recompose();
    assert_eq!(places(&lay_out(&composition, &mut text))[0].1, 68.0);
}

/// A block inside content composed while measuring updates when its state changes, and the
/// layout that holds it is never served from the cache.
#[test]
fn blocks_in_subcomposed_content_update() {
    let state = Rc::new(RefCell::new(None::<State<i32>>));
    let kept = Rc::clone(&state);
    let mut composition = composition(move || {
        let number = remember(|| 0);
        *kept.borrow_mut() = Some(number.clone());
        restartable((), move || {
            let number = number.clone();
            subcompose_layout(Modifier::new(), move |scope, constraints| {
                let number = number.clone();
                let indices = scope.subcompose(0_u8, move || {
                    let number = number.clone();
                    restartable((), move || label(format!("n {}", number.get())));
                });
                let size = scope.measure(indices[0], constraints);
                SubcomposeResult {
                    size,
                    placements: vec![(indices[0], Offset::default())],
                }
            });
        });
    });
    composition.compose();
    let mut text = Counting::default();
    assert_eq!(places(&lay_out(&composition, &mut text))[0].0, "n 0");
    state.borrow().clone().unwrap().set(7);
    composition.recompose();
    assert_eq!(places(&lay_out(&composition, &mut text))[0].0, "n 7");
}
