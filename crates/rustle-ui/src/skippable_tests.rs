//! Pure composition tests for shared skippable output and its invalidation.

use std::cell::Cell;

use rustle_runtime::{disposable_effect, remember, side_effect};

use super::*;

/// Emits a text node whose content identifies its position in collected output.
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

/// Returns text labels from flat output, rejecting unexpected node kinds.
fn labels(nodes: &[Node]) -> Vec<&str> {
    nodes
        .iter()
        .map(|node| match &node.content {
            Content::Text { content, .. } => content.as_str(),
            _ => panic!("expected a text node"),
        })
        .collect()
}

/// Returns shared children from a layout node, rejecting other node kinds.
fn children(node: &Node) -> &Rc<Vec<Node>> {
    match &node.content {
        Content::Layout { children, .. } => children,
        _ => panic!("expected a layout node"),
    }
}

/// Equal dependencies reuse the actual cached child allocation across repeated passes.
#[test]
fn cache_hits_share_children_without_cloning_nodes() {
    let runs = Rc::new(Cell::new(0));
    let observed = Rc::clone(&runs);
    let mut composition = composition(move || {
        layout(Modifier::new(), Overlay::default(), || {
            skippable((), || {
                observed.set(observed.get() + 1);
                label("first");
                label("second");
            });
        });
    });
    composition.compose();
    let first = Rc::clone(children(composition.output().unwrap()));
    for _ in 0..4 {
        composition.compose();
        assert!(Rc::ptr_eq(&first, children(composition.output().unwrap())));
    }
    assert_eq!(runs.get(), 1);
    assert_eq!(labels(&first), ["first", "second"]);
}

/// Dependency changes replace cached output, including transitions to and from empty output.
#[test]
fn dependencies_replace_output_without_retaining_removed_nodes() {
    let count = Rc::new(Cell::new(3));
    let input = Rc::clone(&count);
    let mut composition = composition(move || {
        skippable(input.get(), || {
            for index in 0..input.get() {
                label(index);
            }
        });
    });
    composition.compose();
    let previous = Rc::clone(children(composition.output().unwrap()));
    count.set(2);
    composition.compose();
    let current = children(composition.output().unwrap());
    assert!(!Rc::ptr_eq(&previous, current));
    assert_eq!(labels(current), ["0", "1"]);
    assert_eq!(labels(&previous), ["0", "1", "2"]);
    count.set(0);
    composition.compose();
    assert!(children(composition.output().unwrap()).is_empty());
    composition.compose();
    assert!(children(composition.output().unwrap()).is_empty());
    count.set(2);
    composition.compose();
    assert_eq!(labels(children(composition.output().unwrap())), ["0", "1"]);
}

/// Sibling emission preserves order and copy-on-write never appends siblings to cached output.
#[test]
fn cached_batches_preserve_order_and_do_not_accumulate_siblings() {
    let suffix = Rc::new(Cell::new(0));
    let input = Rc::clone(&suffix);
    let mut composition = rustle_runtime::Composition::new(move || {
        collect(|| {
            skippable((), || {
                label("cached-a");
                label("cached-b");
            });
            label(input.get());
            skippable((), || label("last"));
        })
    });
    for index in 0..4 {
        suffix.set(index);
        composition.compose();
        assert_eq!(
            labels(composition.output().unwrap()),
            ["cached-a", "cached-b", &index.to_string(), "last"]
        );
    }
}

/// Nested skipped scopes stay isolated and inner cached batches survive outer invalidation.
#[test]
fn nested_scopes_preserve_inner_cache_and_collection_boundaries() {
    let generation = Rc::new(Cell::new(0));
    let input = Rc::clone(&generation);
    let inner_runs = Rc::new(Cell::new(0));
    let observed = Rc::clone(&inner_runs);
    let mut composition = rustle_runtime::Composition::new(move || {
        collect(|| {
            label("before");
            skippable(input.get(), || {
                label(input.get());
                let isolated = collect(|| {
                    skippable((), || {
                        observed.set(observed.get() + 1);
                        label("inner");
                    });
                });
                assert_eq!(labels(&isolated), ["inner"]);
                for node in isolated {
                    emit(node);
                }
            });
            label("after");
        })
    });
    for index in [0, 0, 1, 1, 2] {
        generation.set(index);
        composition.compose();
        assert_eq!(
            labels(composition.output().unwrap()),
            ["before", &index.to_string(), "inner", "after"]
        );
    }
    assert_eq!(inner_runs.get(), 1);
}

/// Reads inside a nested cache invalidate its ancestors while remembered state survives hits.
#[test]
fn state_reads_invalidate_nested_cached_output() {
    let state = Rc::new(RefCell::new(None));
    let handle = Rc::clone(&state);
    let runs = Rc::new(Cell::new(0));
    let observed = Rc::clone(&runs);
    let mut composition = rustle_runtime::Composition::new(move || {
        collect(|| {
            skippable((), || {
                skippable((), || {
                    let value = remember(|| 0);
                    *handle.borrow_mut() = Some(value.clone());
                    observed.set(observed.get() + 1);
                    label(value.get());
                });
            });
        })
    });
    composition.compose();
    composition.compose();
    assert_eq!(runs.get(), 1);
    state.borrow().as_ref().unwrap().set(7);
    assert!(composition.recompose());
    assert_eq!(labels(composition.output().unwrap()), ["7"]);
    assert_eq!(runs.get(), 2);
    composition.compose();
    assert_eq!(runs.get(), 2);
}

/// A changed provided local cannot leave old cached text visible.
#[test]
fn composition_local_changes_refresh_cached_output() {
    let direction = Rc::new(Cell::new(LayoutDirection::Ltr));
    let input = Rc::clone(&direction);
    let mut composition = rustle_runtime::Composition::new(move || {
        collect(|| {
            LOCAL_LAYOUT_DIRECTION.provide(input.get(), || {
                skippable((), || label(if is_rtl() { "rtl" } else { "ltr" }));
            });
        })
    });
    composition.compose();
    composition.compose();
    assert_eq!(labels(composition.output().unwrap()), ["ltr"]);
    direction.set(LayoutDirection::Rtl);
    composition.compose();
    assert_eq!(labels(composition.output().unwrap()), ["rtl"]);
}

/// Cache hits do not rerun effects, and removing the cached group still disposes its effect.
#[test]
fn skipped_effects_stay_live_until_the_group_is_removed() {
    let visible = Rc::new(Cell::new(true));
    let input = Rc::clone(&visible);
    let effects = Rc::new(Cell::new(0));
    let effect_runs = Rc::clone(&effects);
    let cleanups = Rc::new(Cell::new(0));
    let cleanup_runs = Rc::clone(&cleanups);
    let mut composition = composition(move || {
        if input.get() {
            skippable((), || {
                let effects = Rc::clone(&effect_runs);
                side_effect(move || effects.set(effects.get() + 1));
                let cleanups = Rc::clone(&cleanup_runs);
                disposable_effect((), move || move || cleanups.set(cleanups.get() + 1));
                label("visible");
            });
        }
    });
    composition.compose();
    composition.compose();
    assert_eq!(effects.get(), 1);
    assert_eq!(cleanups.get(), 0);
    visible.set(false);
    composition.compose();
    assert_eq!(cleanups.get(), 1);
    assert!(children(composition.output().unwrap()).is_empty());
    visible.set(true);
    composition.compose();
    assert_eq!(effects.get(), 2);
    drop(composition);
    assert_eq!(cleanups.get(), 2);
}

/// A panicking nested collector restores its parent, even after adopting a shared batch.
#[test]
fn unwinding_restores_the_parent_scope() {
    let nodes = collect(|| {
        label("before");
        let result = std::panic::catch_unwind(|| {
            collect_scope(|| {
                SCOPES.with(|scopes| {
                    scopes
                        .borrow_mut()
                        .last_mut()
                        .unwrap()
                        .append(Rc::new(vec![Node {
                            modifier: Modifier::new(),
                            content: Content::Text {
                                content: "discarded".to_owned(),
                                style: TextStyle::default(),
                                editing: None,
                            },
                        }]));
                });
                panic!("discard this scope");
            });
        });
        assert!(result.is_err());
        label("after");
    });
    assert_eq!(labels(&nodes), ["before", "after"]);
    SCOPES.with(|scopes| assert!(scopes.borrow().is_empty()));
}

/// Replacing a cache releases its previous allocation when no output snapshot retains it.
#[test]
fn invalidation_releases_the_previous_cached_batch() {
    let generation = Rc::new(Cell::new(0));
    let input = Rc::clone(&generation);
    let mut composition = composition(move || {
        skippable(input.get(), || {
            label(input.get());
            label("second");
        });
    });
    composition.compose();
    let previous = Rc::downgrade(children(composition.output().unwrap()));
    generation.set(1);
    composition.compose();
    assert!(previous.upgrade().is_none());
    assert_eq!(
        labels(children(composition.output().unwrap())),
        ["1", "second"]
    );
}
