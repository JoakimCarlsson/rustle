//! Tests of scoped recomposition, effects, locals, snapshots and derived state, by run counts.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use super::*;

/// A shared counter.
fn counter() -> Rc<Cell<usize>> {
    Rc::default()
}

/// A shared log of events.
fn log() -> Rc<RefCell<Vec<String>>> {
    Rc::default()
}

/// A place to hand state made during composition to the test.
fn slot<T>() -> Rc<RefCell<Option<State<T>>>> {
    Rc::new(RefCell::new(None))
}

/// The state stored in `slot`.
fn taken<T: 'static>(slot: &Rc<RefCell<Option<State<T>>>>) -> State<T> {
    slot.borrow().clone().expect("state was made")
}

/// State changed inside a nested scope reruns only that scope; the outer one reruns for its own
/// state with the inner skipped, and a scope changed twice runs once.
#[test]
fn nested_scopes_run_exactly_as_often_as_needed() {
    let (outer_runs, inner_runs, root_runs) = (counter(), counter(), counter());
    let (outer_slot, inner_slot) = (slot::<i32>(), slot::<i32>());
    let (o, i, r, os, is) = (
        outer_runs.clone(),
        inner_runs.clone(),
        root_runs.clone(),
        outer_slot.clone(),
        inner_slot.clone(),
    );
    let mut composition = Composition::new(move || {
        r.set(r.get() + 1);
        let outer = remember(|| 0);
        let inner = remember(|| 0);
        *os.borrow_mut() = Some(outer.clone());
        *is.borrow_mut() = Some(inner.clone());
        let (o, i) = (o.clone(), i.clone());
        restartable((), move || {
            o.set(o.get() + 1);
            let _ = outer.get();
            let (i, inner) = (i.clone(), inner.clone());
            restartable((), move || {
                i.set(i.get() + 1);
                inner.get()
            });
        });
    });
    composition.compose();
    assert_eq!(
        (root_runs.get(), outer_runs.get(), inner_runs.get()),
        (1, 1, 1)
    );

    taken(&inner_slot).set(1);
    assert!(composition.recompose());
    assert_eq!(
        (root_runs.get(), outer_runs.get(), inner_runs.get()),
        (1, 1, 2)
    );

    taken(&outer_slot).set(1);
    assert!(composition.recompose());
    assert_eq!(
        (root_runs.get(), outer_runs.get(), inner_runs.get()),
        (1, 2, 2)
    );

    taken(&outer_slot).set(2);
    taken(&inner_slot).set(2);
    taken(&inner_slot).set(3);
    assert!(composition.recompose());
    assert_eq!(
        (root_runs.get(), outer_runs.get(), inner_runs.get()),
        (1, 3, 3)
    );
    assert!(!composition.recompose());
}

/// Items of a keyed list keep their state and their scopes through reordering, only the item
/// whose state changed reruns, and a removed item cleans up and is ignored afterwards.
#[test]
fn keyed_list_items_keep_identity_and_clean_up() {
    let list = slot::<Vec<u32>>();
    let handles: Rc<RefCell<HashMap<u32, State<i32>>>> = Rc::default();
    let runs: Rc<RefCell<HashMap<u32, usize>>> = Rc::default();
    let events = log();
    let (ls, hs, rs, ev) = (list.clone(), handles.clone(), runs.clone(), events.clone());
    let mut composition = Composition::new(move || {
        let items = remember(|| vec![1, 2, 3]);
        *ls.borrow_mut() = Some(items.clone());
        let (hs, rs, ev) = (hs.clone(), rs.clone(), ev.clone());
        restartable((), move || {
            for id in items.get() {
                let (hs, rs, ev) = (hs.clone(), rs.clone(), ev.clone());
                key(id, move || {
                    restartable(id, move || {
                        *rs.borrow_mut().entry(id).or_default() += 1;
                        let own = remember(|| 0);
                        hs.borrow_mut().insert(id, own.clone());
                        let (ev, own_value) = (ev.clone(), own.get());
                        disposable_effect(id, move || {
                            ev.borrow_mut().push(format!("start {id}"));
                            move || ev.borrow_mut().push(format!("stop {id}"))
                        });
                        own_value
                    });
                });
            }
        });
    });
    composition.compose();
    assert_eq!(*events.borrow(), ["start 1", "start 2", "start 3"]);

    handles.borrow()[&2].set(7);
    composition.recompose();
    assert_eq!(runs.borrow()[&1], 1);
    assert_eq!(runs.borrow()[&2], 2);
    assert_eq!(runs.borrow()[&3], 1);

    taken(&list).set(vec![3, 2, 1]);
    composition.recompose();
    assert_eq!(
        (runs.borrow()[&1], runs.borrow()[&2], runs.borrow()[&3]),
        (1, 2, 1)
    );
    assert_eq!(handles.borrow()[&2].get(), 7);
    assert_eq!(events.borrow().len(), 3);

    let stale = handles.borrow()[&1].clone();
    taken(&list).set(vec![3, 2, 4]);
    composition.recompose();
    assert_eq!(
        events.borrow()[3..],
        ["stop 1", "start 4"].map(String::from)
    );
    stale.set(5);
    composition.recompose();
    assert_eq!(runs.borrow()[&1], 1);
    assert_eq!(runs.borrow()[&4], 1);
    assert_eq!(runs.borrow()[&2], 2);
    assert_eq!(handles.borrow()[&2].get(), 7);
    assert_eq!(handles.borrow()[&4].get(), 0);
}

/// A scope below an untaken `if` leaves the composition, runs its cleanup once, and starts
/// afresh with new state when it returns.
#[test]
fn conditional_scopes_dispose_and_restart() {
    let show = slot::<bool>();
    let events = log();
    let (sh, ev) = (show.clone(), events.clone());
    let mut composition = Composition::new(move || {
        let visible = remember(|| true);
        *sh.borrow_mut() = Some(visible.clone());
        let ev = ev.clone();
        restartable((), move || {
            if visible.get() {
                let ev = ev.clone();
                restartable((), move || {
                    let count = remember(|| 0);
                    count.update(|value| *value += 1);
                    ev.borrow_mut().push(format!("body {}", count.get()));
                    let ev = ev.clone();
                    disposable_effect((), move || {
                        ev.borrow_mut().push("start".into());
                        move || ev.borrow_mut().push("stop".into())
                    });
                });
            }
        });
    });
    composition.compose();
    taken(&show).set(false);
    composition.recompose();
    taken(&show).set(true);
    composition.recompose();
    assert_eq!(
        events.borrow()[..],
        ["body 1", "start", "stop", "body 1", "start"].map(String::from)
    );
}

/// A disposable effect whose key changes cleans up before it starts again, inside a scope that
/// reruns alone, and side effects run after each scope run.
#[test]
fn effects_inside_a_rerunning_scope_order_cleanup_before_start() {
    let state = slot::<i32>();
    let events = log();
    let (st, ev) = (state.clone(), events.clone());
    let mut composition = Composition::new(move || {
        let value = remember(|| 0);
        *st.borrow_mut() = Some(value.clone());
        let ev = ev.clone();
        restartable((), move || {
            let current = value.get();
            let (starts, stops, after) = (ev.clone(), ev.clone(), ev.clone());
            disposable_effect(current / 2, move || {
                starts.borrow_mut().push(format!("start {}", current / 2));
                move || stops.borrow_mut().push(format!("stop {}", current / 2))
            });
            side_effect(move || after.borrow_mut().push(format!("after {current}")));
        });
    });
    composition.compose();
    for next in 1..=2 {
        taken(&state).set(next);
        composition.recompose();
    }
    assert_eq!(
        events.borrow()[..],
        [
            "start 0", "after 0", "after 1", "stop 0", "start 1", "after 2"
        ]
        .map(String::from)
    );
}

/// Dropping a composition runs the cleanups of everything still in it.
#[test]
fn dropping_a_composition_runs_cleanups() {
    let cleaned = counter();
    let c = cleaned.clone();
    let mut composition = Composition::new(move || {
        let c = c.clone();
        restartable((), move || {
            let c = c.clone();
            disposable_effect((), move || move || c.set(c.get() + 1));
        });
    });
    composition.compose();
    assert_eq!(cleaned.get(), 0);
    drop(composition);
    assert_eq!(cleaned.get(), 1);
}

/// A launched effect is cancelled when its call leaves the composition and stops at its key.
#[test]
fn launched_effects_are_cancelled_with_their_scope() {
    let show = slot::<bool>();
    let ticks = counter();
    let (sh, t) = (show.clone(), ticks.clone());
    let mut composition = Composition::new(move || {
        let visible = remember(|| true);
        *sh.borrow_mut() = Some(visible.clone());
        let t = t.clone();
        restartable((), move || {
            if visible.get() {
                let t = t.clone();
                launched_effect((), move |scope| async move {
                    loop {
                        scope.next_frame().await;
                        t.set(t.get() + 1);
                    }
                });
            }
        });
    });
    composition.compose();
    for frame in 1..=3 {
        composition.run_frame(Duration::from_millis(16 * frame));
    }
    assert_eq!(ticks.get(), 3);
    taken(&show).set(false);
    composition.recompose();
    for frame in 4..=6 {
        composition.run_frame(Duration::from_millis(16 * frame));
    }
    assert_eq!(ticks.get(), 3);
}

static LOCAL_NUMBER: CompositionLocal<i32> = CompositionLocal::new(|| 0);

/// A scope sees the locals of where it was composed when it reruns alone, and reruns when the
/// provided value changes even though its deps are equal.
#[test]
fn locals_reach_scopes_and_invalidate_them() {
    let provided = slot::<i32>();
    let own = slot::<i32>();
    let seen = log();
    let (pr, ow, sn) = (provided.clone(), own.clone(), seen.clone());
    let mut composition = Composition::new(move || {
        let value = remember(|| 1);
        let local_own = remember(|| 0);
        *pr.borrow_mut() = Some(value.clone());
        *ow.borrow_mut() = Some(local_own.clone());
        let sn = sn.clone();
        LOCAL_NUMBER.provide(value.get(), || {
            restartable((), move || {
                let _ = local_own.get();
                sn.borrow_mut().push(format!("{}", LOCAL_NUMBER.current()));
            });
        });
    });
    composition.compose();
    taken(&own).set(1);
    composition.recompose();
    taken(&provided).set(2);
    composition.recompose();
    taken(&own).set(2);
    composition.recompose();
    assert_eq!(seen.borrow()[..], ["1", "1", "2", "2"].map(String::from));
}

/// A memo that is skipped inside a scope still keeps the scope subscribed to what it read.
#[test]
fn a_skipped_memo_keeps_its_scope_subscribed() {
    let (a, b) = (slot::<i32>(), slot::<i32>());
    let (scope_runs, other_runs) = (counter(), counter());
    let (aa, bb, sr, or) = (a.clone(), b.clone(), scope_runs.clone(), other_runs.clone());
    let mut composition = Composition::new(move || {
        let (watched, trigger) = (remember(|| 0), remember(|| 0));
        *aa.borrow_mut() = Some(watched.clone());
        *bb.borrow_mut() = Some(trigger.clone());
        let (sr, or, w2) = (sr.clone(), or.clone(), watched.clone());
        restartable((), move || {
            sr.set(sr.get() + 1);
            let _ = trigger.get();
            let w = watched.clone();
            memo((), move || w.get());
        });
        restartable((), move || {
            or.set(or.get() + 1);
            w2.get()
        });
    });
    composition.compose();
    taken(&b).set(1);
    composition.recompose();
    assert_eq!((scope_runs.get(), other_runs.get()), (2, 1));
    taken(&a).set(1);
    composition.recompose();
    assert_eq!((scope_runs.get(), other_runs.get()), (3, 2));
}

/// A derived state that is not recalculated in a scope's run still keeps the scope subscribed.
#[test]
fn a_cached_derived_state_keeps_its_scope_subscribed() {
    let (a, b) = (slot::<i32>(), slot::<i32>());
    let (scope_runs, other_runs) = (counter(), counter());
    let (aa, bb, sr, or) = (a.clone(), b.clone(), scope_runs.clone(), other_runs.clone());
    let mut composition = Composition::new(move || {
        let (watched, trigger) = (remember(|| 0), remember(|| 0));
        *aa.borrow_mut() = Some(watched.clone());
        *bb.borrow_mut() = Some(trigger.clone());
        let (sr, or, w2) = (sr.clone(), or.clone(), watched.clone());
        restartable((), move || {
            sr.set(sr.get() + 1);
            let _ = trigger.get();
            let w = watched.clone();
            derived_state_of(move || w.get() * 2).get()
        });
        restartable((), move || {
            or.set(or.get() + 1);
            w2.get()
        });
    });
    composition.compose();
    taken(&b).set(1);
    composition.recompose();
    assert_eq!((scope_runs.get(), other_runs.get()), (2, 1));
    taken(&a).set(1);
    composition.recompose();
    assert_eq!((scope_runs.get(), other_runs.get()), (3, 2));
}

/// Derived state recalculates only when its inputs change and follows them.
#[test]
fn derived_state_follows_inputs() {
    let input = slot::<i32>();
    let calculations = counter();
    let shown = Rc::new(Cell::new(0));
    let (inp, calc, sh) = (input.clone(), calculations.clone(), shown.clone());
    let mut composition = Composition::new(move || {
        let value = remember(|| 1);
        *inp.borrow_mut() = Some(value.clone());
        let other = remember(|| 0);
        let (calc, sh) = (calc.clone(), sh.clone());
        let source = value.clone();
        let doubled = derived_state_of(move || {
            calc.set(calc.get() + 1);
            source.get() * 2
        });
        sh.set(doubled.get() + other.get());
    });
    composition.compose();
    assert_eq!((calculations.get(), shown.get()), (1, 2));
    composition.invalidate();
    composition.recompose();
    assert_eq!((calculations.get(), shown.get()), (1, 2));
    taken(&input).set(5);
    composition.recompose();
    assert_eq!((calculations.get(), shown.get()), (2, 10));
}

/// Snapshot-aware state is frozen in a read snapshot and held back in a mutable one, and a
/// state made by `remember_live` ignores snapshots.
#[test]
fn snapshots_freeze_hold_back_and_conflict() {
    let slots = (slot::<i32>(), slot::<String>(), slot::<i32>());
    let (a, b, c) = (slots.0.clone(), slots.1.clone(), slots.2.clone());
    let mut composition = Composition::new(move || {
        *a.borrow_mut() = Some(remember(|| 1));
        *b.borrow_mut() = Some(mutable_state_of("one".to_string()));
        *c.borrow_mut() = Some(remember_live(|| 1));
    });
    composition.compose();
    let (number, text, live) = (taken(&slots.0), taken(&slots.1), taken(&slots.2));

    let frozen = Snapshot::take_snapshot();
    number.set(2);
    text.set("two".into());
    live.set(2);
    frozen.enter(|| {
        assert_eq!(number.get(), 1);
        assert_eq!(text.get(), "one");
        assert_eq!(live.get(), 2);
    });
    assert_eq!((number.get(), text.get(), live.get()), (2, "two".into(), 2));
    frozen.dispose();

    let mut writer = Snapshot::take_mutable_snapshot();
    writer.enter(|| {
        number.set(10);
        number.update(|value| *value += 1);
        text.set("ten".into());
        assert_eq!(number.get(), 11);
    });
    assert_eq!((number.get(), text.get()), (2, "two".into()));

    let mut rival = Snapshot::take_mutable_snapshot();
    rival.enter(|| number.set(99));
    assert_eq!(rival.apply(), ApplyResult::Success);
    assert_eq!(number.get(), 99);
    assert_eq!(writer.apply(), ApplyResult::Failure);
    assert_eq!((number.get(), text.get()), (99, "two".into()));
}

/// Apply observers hear exactly the states an applied snapshot wrote, once.
#[test]
fn apply_observers_see_applied_writes() {
    let slots = (slot::<i32>(), slot::<i32>());
    let (a, b) = (slots.0.clone(), slots.1.clone());
    let mut composition = Composition::new(move || {
        *a.borrow_mut() = Some(remember(|| 0));
        *b.borrow_mut() = Some(remember(|| 0));
    });
    composition.compose();
    let (first, second) = (taken(&slots.0), taken(&slots.1));
    let heard = Rc::new(RefCell::new(Vec::<(usize, bool, bool)>::new()));
    let (h, f, s) = (heard.clone(), first.clone(), second.clone());
    let _handle = Snapshot::register_apply_observer(move |changes| {
        h.borrow_mut()
            .push((changes.len(), changes.contains(&f), changes.contains(&s)));
    });
    Snapshot::with_mutable_snapshot(|| {
        first.set(1);
        second.set(2);
    });
    assert_eq!(*heard.borrow(), [(2, true, true)]);
    second.set(3);
    assert_eq!(heard.borrow().last(), Some(&(1, false, true)));
    let mut outer = Snapshot::take_mutable_snapshot();
    outer.enter(|| {
        let mut inner = Snapshot::take_mutable_snapshot();
        inner.enter(|| first.set(7));
        assert_eq!(inner.apply(), ApplyResult::Success);
        assert_eq!(first.get(), 7);
    });
    assert_eq!(first.get(), 1);
    assert_eq!(heard.borrow().len(), 2);
    assert_eq!(outer.apply(), ApplyResult::Success);
    assert_eq!(first.get(), 7);
    assert_eq!(heard.borrow().len(), 3);
}

/// A snapshot flow yields once at first, then only when what it read changed and the result
/// differs, and never runs its block when nothing changed.
#[test]
fn snapshot_flow_yields_distinct_changes() {
    let input = slot::<i32>();
    let i = input.clone();
    let mut composition = Composition::new(move || {
        *i.borrow_mut() = Some(remember(|| 1));
    });
    composition.compose();
    let value = taken(&input);
    let runs = counter();
    let (state, r) = (value.clone(), runs.clone());
    let mut flow = snapshot_flow(move || {
        r.set(r.get() + 1);
        state.get() / 10
    });
    assert_eq!(flow.poll(), Some(0));
    assert_eq!(flow.poll(), None);
    assert_eq!(runs.get(), 1);
    value.set(5);
    assert_eq!(flow.poll(), None);
    assert_eq!(runs.get(), 2);
    value.set(25);
    assert_eq!(flow.poll(), Some(2));
    assert_eq!(flow.poll(), None);
    assert_eq!(runs.get(), 3);
}

/// `remember` takes any cloneable value and keeps one value per call site, while
/// `remember_live` holds a value that cannot be cloned.
#[test]
fn remember_keeps_cloneable_and_live_values() {
    struct NoClone(i32);
    let handles = (slot::<Vec<i32>>(), slot::<NoClone>());
    let (a, b) = (handles.0.clone(), handles.1.clone());
    let inits = counter();
    let ic = inits.clone();
    let mut composition = Composition::new(move || {
        let ic = ic.clone();
        *a.borrow_mut() = Some(remember(move || {
            ic.set(ic.get() + 1);
            vec![1, 2]
        }));
        *b.borrow_mut() = Some(remember_live(|| NoClone(4)));
    });
    composition.compose();
    taken(&handles.0).update(|list| list.push(3));
    taken(&handles.1).update(|value| value.0 += 1);
    composition.recompose();
    composition.invalidate();
    composition.recompose();
    assert_eq!(taken(&handles.0).get(), [1, 2, 3]);
    assert_eq!(taken(&handles.1).with(|value| value.0), 5);
    assert_eq!(inits.get(), 1);
}
