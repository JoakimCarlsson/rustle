//! Reusing what an earlier paint drew for a subtree that did not change.
//!
//! As `gpui` reuses the prepaint and paint of a cached view, a measured subtree kept from an
//! earlier frame (a [`Paint::Shared`] node, which layout reuses while nothing it read changed)
//! is painted once and its share of every list of the display list is noted. The next paint
//! that meets the same subtree, in the same circumstances and with nothing its drawing read
//! changed, moves those entries over from the list painted before, moved to where the subtree
//! is now, instead of painting it again. A subtree that changed is painted again, and the
//! unchanged subtrees inside it are reused in turn, so a caret or a spinner paints only its
//! own subtree and the path to it.

use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::DrawCommand;
use rustle_runtime::{Reads, track_reads};

use super::super::measured::Measured;
use super::super::regions::DisplayList;
use super::Painter;
use crate::interaction::resolve;

/// How far apart two edges may be and still count as the same, for edges worked out in
/// floating point along different paths.
const SLACK: f32 = 1e-3;

macro_rules! lengths {
    ($($list:ident),* $(,)?) => {
        /// How many entries each list of a display list held at some point of painting, and
        /// how many calls of placement handlers had been noted.
        #[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
        pub(crate) struct Lengths {
            $(pub(crate) $list: usize,)*
            pub(crate) placed: usize,
        }

        impl Lengths {
            /// The lengths of the lists of `list` now, with `placed` calls noted.
            pub(crate) fn of(list: &DisplayList, placed: usize) -> Self {
                Self {
                    $($list: list.$list.len(),)*
                    placed,
                }
            }

            /// These lengths, counted from `from`, counted from `to` instead.
            fn moved(self, from: Self, to: Self) -> Self {
                Self {
                    $($list: self.$list - from.$list + to.$list,)*
                    placed: self.placed - from.placed + to.placed,
                }
            }
        }
    };
}

lengths!(
    commands,
    regions,
    scrolls,
    focusables,
    test_tags,
    presses,
    buttons,
    key_handlers,
    drags,
    hovers,
    long_presses,
    pointer_moves,
    drag_sources,
    drop_targets,
    drag_and_drop_sources,
    drag_and_drop_targets,
    file_drops,
    pointer_inputs,
    pinches,
    blockers,
    cursors,
    ime_carets,
    interactions,
    semantics,
    popups,
    pointer_events,
    text_inputs,
);

/// A call of a placement handler made while painting, kept so a reused subtree makes it again
/// where the subtree is now.
#[derive(Clone)]
pub(crate) enum PlacedCall {
    /// An [`on_placed`](crate::Modifier::on_placed) handler told the area's place.
    Placed(Rc<dyn Fn(Rect, Size)>, Rect),
    /// A handler told the visible part of the area.
    Visible(Rc<dyn Fn(Rect)>, Rect),
}

impl PlacedCall {
    /// Makes the call again, moved by `delta`, in a window of size `window`.
    fn call(&self, delta: Offset, window: Size) -> Self {
        match self {
            Self::Placed(handler, rect) => {
                let rect = moved_rect(*rect, delta);
                handler(rect, window);
                Self::Placed(Rc::clone(handler), rect)
            }
            Self::Visible(handler, rect) => {
                let rect = moved_rect(*rect, delta);
                handler(rect);
                Self::Visible(Rc::clone(handler), rect)
            }
        }
    }
}

/// What the painting of a subtree depends on, besides the subtree itself, that the painter
/// carries down to it.
#[derive(Clone, PartialEq)]
pub(crate) struct Context {
    /// How opaque the alpha modifiers around it make it.
    alpha: f32,
    /// How many enclosing semantics drop every description inside.
    clearing: usize,
    /// The id of the focus of the nearest focusable around it.
    focus: Option<u64>,
    /// The focused flag given from outside around it, by address.
    focused: Option<usize>,
    /// The drag-end handler waiting for the drag handler inside it, by address.
    pending_end: Option<usize>,
    /// The visible rectangle of the interaction group around it, from the subtree's place.
    group: Option<Rect>,
    /// How large the window is.
    window: Size,
    /// The grid edges are rounded to.
    grid: Option<f32>,
    /// The clips around it, outermost first.
    clips: Vec<Rect>,
    /// The semantics it is inside, innermost last.
    parents: Vec<usize>,
    /// The scrolling areas it is inside, outermost first.
    scrolls: Vec<usize>,
    /// The areas hearing pointer events it is inside, innermost last.
    pointer_events: Vec<usize>,
}

impl Context {
    /// Whether a subtree painted in `self` paints the same in `other`, once the indices of the
    /// stacks around it are mapped and it is moved to where it is in `other`.
    fn fits(&self, other: &Self) -> bool {
        self.alpha == other.alpha
            && self.clearing == other.clearing
            && self.focus == other.focus
            && self.focused == other.focused
            && same_rect(self.group, other.group)
            && self.window == other.window
            && self.grid == other.grid
            && self.parents.len() == other.parents.len()
            && self.scrolls.len() == other.scrolls.len()
            && self.pointer_events.len() == other.pointer_events.len()
    }
}

/// What one painted subtree left in the display list, kept to reuse it.
pub(crate) struct Record {
    /// The measured subtree, held so its address names it while the record lives.
    _measured: Weak<Measured>,
    /// Where it was painted, unrounded.
    origin: Offset,
    /// What it was painted in.
    context: Context,
    /// Whether a clip around it cut one of its areas.
    cut: bool,
    /// Whether a drag handler inside it took a drag-end handler, which may be the one
    /// waiting around it.
    used_end: bool,
    /// The union of every area it cut to the clips, in the window.
    extent: Option<Rect>,
    /// The lengths of the lists when it began.
    start: Lengths,
    /// The lengths of the lists when it was done.
    end: Lengths,
    /// Where it said text input goes, if it did.
    ime_area: Option<Rect>,
    /// What its painting read.
    reads: Reads,
}

/// What a display list keeps of how it was painted, for the next paint to reuse.
#[derive(Default)]
pub(crate) struct Reuse {
    /// The record of every subtree painted or reused, by the address of its measurement.
    records: HashMap<usize, Record>,
    /// The calls of placement handlers made while painting, in order.
    placed: Vec<PlacedCall>,
    /// How many subtrees were reused.
    pub(crate) reused: usize,
    /// How many kept subtrees were painted afresh.
    pub(crate) repainted: usize,
}

/// The list painted before and what it keeps, consumed as its subtrees are reused.
pub(crate) struct Previous {
    list: DisplayList,
    records: HashMap<usize, Record>,
    placed: Vec<PlacedCall>,
}

impl Previous {
    /// Takes `list` apart into its entries and its records.
    pub(crate) fn new(mut list: DisplayList) -> Option<Self> {
        let reuse = std::mem::take(&mut list.reuse);
        (!reuse.records.is_empty()).then_some(Self {
            list,
            records: reuse.records,
            placed: reuse.placed,
        })
    }
}

/// What the paint under way keeps for the next one.
#[derive(Default)]
pub(crate) struct Kept {
    records: HashMap<usize, Record>,
    /// Subtrees met more than once in this paint, which are not kept.
    doubled: HashSet<usize>,
    placed: Vec<PlacedCall>,
    reused: usize,
    repainted: usize,
}

impl Kept {
    /// How many calls of placement handlers were noted.
    pub(crate) fn placed_len(&self) -> usize {
        self.placed.len()
    }

    /// Notes a call of a placement handler.
    pub(crate) fn note(&mut self, call: PlacedCall) {
        self.placed.push(call);
    }

    /// What the list painted keeps.
    pub(crate) fn into_reuse(self) -> Reuse {
        Reuse {
            records: self.records,
            placed: self.placed,
            reused: self.reused,
            repainted: self.repainted,
        }
    }

    /// Keeps `record` under `key`, unless the same subtree was met before in this paint.
    fn keep(&mut self, key: usize, record: Record) {
        if self.doubled.contains(&key) || self.records.remove(&key).is_some() {
            self.doubled.insert(key);
            return;
        }
        self.records.insert(key, record);
    }
}

/// How the entries of a reused subtree change on their way from the list painted before.
struct Shift<'c> {
    /// How far the subtree moved.
    delta: Offset,
    /// The lengths of the old list where the subtree began.
    from: Lengths,
    /// The lengths of the new list where it begins.
    to: Lengths,
    /// What the subtree was painted in before.
    old: &'c Context,
    /// What it is painted in now.
    new: &'c Context,
}

impl Shift<'_> {
    /// `offset` moved along.
    fn offset(&self, offset: Offset) -> Offset {
        Offset::new(offset.x + self.delta.x, offset.y + self.delta.y)
    }

    /// `rect` moved along.
    fn rect(&self, rect: Rect) -> Rect {
        moved_rect(rect, self.delta)
    }

    /// `index`, counted in the old list from `from`, counted in the new one from `to`, or,
    /// when it is before the subtree, the entry in `new` at the place `old` had it.
    fn index(&self, index: usize, from: usize, to: usize, old: &[usize], new: &[usize]) -> usize {
        if index >= from {
            return index - from + to;
        }
        old.iter()
            .position(|&outer| outer == index)
            .and_then(|at| new.get(at).copied())
            .unwrap_or(index)
    }

    /// The index of a semantics node.
    fn parent(&self, index: usize) -> usize {
        self.index(
            index,
            self.from.semantics,
            self.to.semantics,
            &self.old.parents,
            &self.new.parents,
        )
    }

    /// The index of a scrolling area.
    fn scroll(&self, index: usize) -> usize {
        self.index(
            index,
            self.from.scrolls,
            self.to.scrolls,
            &self.old.scrolls,
            &self.new.scrolls,
        )
    }

    /// The index of an area hearing pointer events.
    fn pointer_event(&self, index: usize) -> usize {
        self.index(
            index,
            self.from.pointer_events,
            self.to.pointer_events,
            &self.old.pointer_events,
            &self.new.pointer_events,
        )
    }
}

/// `rect` moved by `delta`.
fn moved_rect(rect: Rect, delta: Offset) -> Rect {
    Rect::new(
        Offset::new(rect.origin.x + delta.x, rect.origin.y + delta.y),
        rect.size,
    )
}

/// Whether `a` and `b` are the same rectangle, give or take [`SLACK`].
fn same_rect(a: Option<Rect>, b: Option<Rect>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            (a.origin.x - b.origin.x).abs() <= SLACK
                && (a.origin.y - b.origin.y).abs() <= SLACK
                && (a.size.width - b.size.width).abs() <= SLACK
                && (a.size.height - b.size.height).abs() <= SLACK
        }
        _ => false,
    }
}

/// Whether `outer` holds all of `inner`, give or take [`SLACK`].
fn holds(outer: Rect, inner: Rect) -> bool {
    inner.origin.x >= outer.origin.x - SLACK
        && inner.origin.y >= outer.origin.y - SLACK
        && inner.origin.x + inner.size.width <= outer.origin.x + outer.size.width + SLACK
        && inner.origin.y + inner.size.height <= outer.origin.y + outer.size.height + SLACK
}

/// The smallest rectangle holding `a` and `b`.
pub(crate) fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let left = a.origin.x.min(b.origin.x);
            let top = a.origin.y.min(b.origin.y);
            let right = (a.origin.x + a.size.width).max(b.origin.x + b.size.width);
            let bottom = (a.origin.y + a.size.height).max(b.origin.y + b.size.height);
            Some(Rect::new(
                Offset::new(left, top),
                Size::new(right - left, bottom - top),
            ))
        }
        (a, None) => a,
        (None, b) => b,
    }
}

/// Clones `from` onto the end of `to`, changing each entry with `change`.
fn append<T: Clone>(to: &mut Vec<T>, from: &[T], change: impl Fn(&mut T)) {
    to.reserve(from.len());
    for entry in from {
        let mut entry = entry.clone();
        change(&mut entry);
        to.push(entry);
    }
}

impl<'a> Painter<'a> {
    /// What painting a subtree at `exact` depends on now, or `None` when it reaches out of the
    /// subtree in a way a reused subtree cannot replay: a focus requester waiting for what is
    /// inside, focus observers waiting for its first focus target, or semantics around it
    /// merging the text inside.
    fn context(&self, exact: Offset) -> Option<Context> {
        if self.requester.is_some()
            || !self.focus_observers.is_empty()
            || !self.merged_text.is_empty()
        {
            return None;
        }
        Some(Context {
            alpha: self.alpha,
            clearing: self.clearing,
            focus: self.focus.as_ref().map(crate::input::Focus::id),
            focused: self
                .focused_flag
                .as_ref()
                .map(|flag| Rc::as_ptr(flag).cast::<()>() as usize),
            pending_end: self
                .pending_end
                .as_ref()
                .map(|end| Rc::as_ptr(end).cast::<()>() as usize),
            group: self.group.map(|group| {
                Rect::new(
                    Offset::new(group.origin.x - exact.x, group.origin.y - exact.y),
                    group.size,
                )
            }),
            window: self.window,
            grid: self.grid,
            clips: self.clips.clone(),
            parents: self.parents.clone(),
            scrolls: self.scroll_parents.clone(),
            pointer_events: self.pointer_event_parents.clone(),
        })
    }

    /// Paints the kept subtree `shared` at `exact`, reusing what the list painted before holds
    /// of it when nothing it depends on changed, and notes what it left for the next paint.
    pub(super) fn paint_shared(&mut self, shared: &'a Rc<Measured>, exact: Offset) {
        let key = Rc::as_ptr(shared) as usize;
        let Some(context) = self.context(exact) else {
            self.paint(shared, exact);
            return;
        };
        if self.reuse(key, &context, exact) {
            return;
        }
        self.kept.repainted += 1;
        let start = Lengths::of(&self.list, self.kept.placed_len());
        let ime_area = self.list.ime_area.take();
        let cut = self.cut.replace(false);
        let used_end = self.used_end.replace(false);
        let extent = self.extent.take();
        let volatile = std::mem::replace(&mut self.volatile, false);
        let ((), reads) = track_reads(|| self.paint(shared, exact));
        let end = Lengths::of(&self.list, self.kept.placed_len());
        let painted_ime_area = self.list.ime_area;
        if painted_ime_area.is_none() {
            self.list.ime_area = ime_area;
        }
        let painted_cut = self.cut.replace(cut || self.cut.get());
        let painted_used_end = self.used_end.replace(used_end || self.used_end.get());
        let painted_extent = self.extent.get();
        self.extent.set(union(extent, painted_extent));
        let painted_volatile = self.volatile || !reads.is_trackable();
        self.volatile = volatile || painted_volatile;
        if painted_volatile {
            return;
        }
        self.kept.keep(
            key,
            Record {
                _measured: Rc::downgrade(shared),
                origin: exact,
                context,
                cut: painted_cut,
                used_end: painted_used_end,
                extent: painted_extent,
                start,
                end,
                ime_area: painted_ime_area,
                reads,
            },
        );
    }

    /// How far the subtree `record` describes moves to be painted at `exact` in `context`, or
    /// `None` when moving it would not paint what painting it there paints: when the clips
    /// around it cut it differently, or it moves off the pixel grid.
    fn delta(&self, record: &Record, context: &Context, exact: Offset) -> Option<Offset> {
        let delta = Offset::new(exact.x - record.origin.x, exact.y - record.origin.y);
        if delta == Offset::default() && record.context.clips == context.clips {
            return Some(delta);
        }
        if record.cut {
            return None;
        }
        if let Some(grid) = self.grid {
            let on_grid = |distance: f32| {
                let steps = distance * grid;
                (steps - steps.round()).abs() <= SLACK
            };
            if !on_grid(delta.x) || !on_grid(delta.y) {
                return None;
            }
        }
        let fits = record.extent.is_none_or(|extent| {
            let extent = moved_rect(extent, delta);
            context.clips.iter().all(|clip| holds(*clip, extent))
        });
        fits.then_some(delta)
    }

    /// Whether the areas drawn by their interaction state in `record`'s share of `list`, moved
    /// by `delta`, are in the state they were painted in.
    fn interactions_hold(&self, list: &DisplayList, record: &Record, delta: Offset) -> bool {
        let base = Lengths::of(&self.list, 0).interactions;
        list.interactions[record.start.interactions..record.end.interactions]
            .iter()
            .enumerate()
            .all(|(at, region)| {
                let rect = moved_rect(region.rect, delta);
                let group = region.group.map(|group| moved_rect(group, delta));
                let now = self
                    .resolved
                    .as_ref()
                    .and_then(|resolved| resolved.get(base + at).copied())
                    .unwrap_or_else(|| {
                        resolve(
                            rect,
                            region.focus.as_ref(),
                            region.focused.as_ref().map(|focused| focused()),
                            group,
                            self.pointer,
                            false,
                        )
                    });
                now == region.interaction
            })
    }

    /// Paints the subtree kept under `key` at `exact` by moving what the list painted before
    /// holds of it, when it was painted in a context that fits `context`, nothing its
    /// painting read changed, and its interactive areas are in the same state. Returns
    /// whether it did.
    fn reuse(&mut self, key: usize, context: &Context, exact: Offset) -> bool {
        if self.kept.records.contains_key(&key) || self.kept.doubled.contains(&key) {
            return false;
        }
        let Some(mut previous) = self.previous.take() else {
            return false;
        };
        let reused = self.reuse_from(&mut previous, key, context, exact);
        self.previous = Some(previous);
        reused
    }

    /// [`reuse`](Self::reuse) with the list painted before taken out of the painter.
    fn reuse_from(
        &mut self,
        previous: &mut Previous,
        key: usize,
        context: &Context,
        exact: Offset,
    ) -> bool {
        let Some(record) = previous.records.remove(&key) else {
            return false;
        };
        if record.used_end && record.context.pending_end != context.pending_end {
            return false;
        }
        if !record.context.fits(context) {
            return false;
        }
        if !record.reads.is_current() {
            return false;
        }
        let Some(delta) = self.delta(&record, context, exact) else {
            return false;
        };
        if !self.interactions_hold(&previous.list, &record, delta) {
            return false;
        }
        let to = Lengths::of(&self.list, self.kept.placed_len());
        let old = record.context.clone();
        let shift = Shift {
            delta,
            from: record.start,
            to,
            old: &old,
            new: context,
        };
        self.move_entries(previous, &record, &shift);
        for call in &previous.placed[record.start.placed..record.end.placed] {
            let call = call.call(delta, self.window);
            self.kept.note(call);
        }
        if let Some(area) = record.ime_area {
            self.list.ime_area = Some(shift.rect(area));
        }
        self.cut.set(self.cut.get() || record.cut);
        self.used_end.set(self.used_end.get() || record.used_end);
        self.extent.set(union(
            self.extent.get(),
            record.extent.map(|extent| shift.rect(extent)),
        ));
        record.reads.merge();
        let moved = Record {
            _measured: record._measured,
            origin: exact,
            context: context.clone(),
            cut: record.cut,
            used_end: record.used_end,
            extent: record.extent.map(|extent| shift.rect(extent)),
            start: to,
            end: record.end.moved(record.start, to),
            ime_area: record.ime_area.map(|area| shift.rect(area)),
            reads: record.reads,
        };
        self.kept.keep(key, moved);
        self.kept.reused += 1;
        true
    }

    /// Moves `record`'s share of every list of the list painted before onto the list being
    /// painted, as `shift` changes it.
    fn move_entries(&mut self, previous: &mut Previous, record: &Record, shift: &Shift) {
        let (from, end, to) = (record.start, record.end, shift.to);
        let old = &mut previous.list;
        let list = &mut self.list;
        let moving = shift.delta != Offset::default();
        list.commands.reserve(end.commands - from.commands);
        for command in &mut old.commands[from.commands..end.commands] {
            let command = std::mem::replace(command, DrawCommand::PopClip);
            list.commands.push(if moving {
                command.translated(shift.delta.x, shift.delta.y)
            } else {
                command
            });
        }
        let slide = |index: usize, from: usize, to: usize| index - from + to;
        append(
            &mut list.regions,
            &old.regions[from.regions..end.regions],
            |region| {
                region.rect = shift.rect(region.rect);
                region.buttons_inside = slide(region.buttons_inside.start, from.buttons, to.buttons)
                    ..slide(region.buttons_inside.end, from.buttons, to.buttons);
            },
        );
        append(
            &mut list.scrolls,
            &old.scrolls[from.scrolls..end.scrolls],
            |region| region.rect = shift.rect(region.rect),
        );
        append(
            &mut list.focusables,
            &old.focusables[from.focusables..end.focusables],
            |region| {
                region.rect = shift.rect(region.rect);
                region.bounds = shift.rect(region.bounds);
                for scroll in &mut region.scrolls {
                    *scroll = shift.scroll(*scroll);
                }
            },
        );
        let focus_ids: Vec<Option<u64>> = old.test_tags[from.test_tags..end.test_tags]
            .iter()
            .map(|tag| {
                tag.focusable
                    .filter(|&at| at < from.focusables)
                    .and_then(|at| old.focusables.get(at))
                    .map(|region| region.focus.id())
            })
            .collect();
        let tags_start = list.test_tags.len();
        append(
            &mut list.test_tags,
            &old.test_tags[from.test_tags..end.test_tags],
            |tag| {
                tag.bounds = shift.rect(tag.bounds);
                tag.visible = shift.rect(tag.visible);
            },
        );
        for (tag, id) in list.test_tags[tags_start..].iter_mut().zip(focus_ids) {
            tag.focusable = match (tag.focusable, id) {
                (Some(at), _) if at >= from.focusables => {
                    Some(slide(at, from.focusables, to.focusables))
                }
                (Some(_), Some(id)) => list.focusables[..to.focusables]
                    .iter()
                    .position(|region| region.focus.id() == id),
                (focusable, _) => focusable,
            };
        }
        append(
            &mut list.presses,
            &old.presses[from.presses..end.presses],
            |region| {
                region.rect = shift.rect(region.rect);
                region.origin = shift.offset(region.origin);
            },
        );
        append(
            &mut list.buttons,
            &old.buttons[from.buttons..end.buttons],
            |region| {
                region.rect = shift.rect(region.rect);
                region.origin = shift.offset(region.origin);
            },
        );
        append(
            &mut list.key_handlers,
            &old.key_handlers[from.key_handlers..end.key_handlers],
            |region| {
                region.rect = shift.rect(region.rect);
                let range = |range: &std::ops::Range<usize>| {
                    slide(range.start, from.focusables, to.focusables)
                        ..slide(range.end, from.focusables, to.focusables)
                };
                region.focusables = range(&region.focusables);
                for raised in &mut region.raised_focusables {
                    *raised = range(raised);
                }
            },
        );
        append(
            &mut list.drags,
            &old.drags[from.drags..end.drags],
            |region| {
                region.rect = shift.rect(region.rect);
                region.origin = shift.offset(region.origin);
                region.inputs_before =
                    slide(region.inputs_before, from.pointer_inputs, to.pointer_inputs);
            },
        );
        append(
            &mut list.hovers,
            &old.hovers[from.hovers..end.hovers],
            |region| {
                region.rect = shift.rect(region.rect);
                region.covered_after = slide(region.covered_after, from.regions, to.regions);
            },
        );
        append(
            &mut list.long_presses,
            &old.long_presses[from.long_presses..end.long_presses],
            |region| region.rect = shift.rect(region.rect),
        );
        append(
            &mut list.pointer_moves,
            &old.pointer_moves[from.pointer_moves..end.pointer_moves],
            |region| {
                region.rect = shift.rect(region.rect);
                region.origin = shift.offset(region.origin);
                region.covered_after = slide(region.covered_after, from.regions, to.regions);
            },
        );
        append(
            &mut list.drag_sources,
            &old.drag_sources[from.drag_sources..end.drag_sources],
            |region| {
                region.rect = shift.rect(region.rect);
                region.commands = slide(region.commands.start, from.commands, to.commands)
                    ..slide(region.commands.end, from.commands, to.commands);
            },
        );
        append(
            &mut list.drop_targets,
            &old.drop_targets[from.drop_targets..end.drop_targets],
            |region| region.rect = shift.rect(region.rect),
        );
        append(
            &mut list.drag_and_drop_sources,
            &old.drag_and_drop_sources[from.drag_and_drop_sources..end.drag_and_drop_sources],
            |region| {
                region.rect = shift.rect(region.rect);
                region.origin = shift.offset(region.origin);
            },
        );
        append(
            &mut list.drag_and_drop_targets,
            &old.drag_and_drop_targets[from.drag_and_drop_targets..end.drag_and_drop_targets],
            |region| region.rect = shift.rect(region.rect),
        );
        append(
            &mut list.file_drops,
            &old.file_drops[from.file_drops..end.file_drops],
            |region| region.rect = shift.rect(region.rect),
        );
        append(
            &mut list.pointer_inputs,
            &old.pointer_inputs[from.pointer_inputs..end.pointer_inputs],
            |region| {
                region.rect = shift.rect(region.rect);
                region.origin = shift.offset(region.origin);
            },
        );
        append(
            &mut list.pinches,
            &old.pinches[from.pinches..end.pinches],
            |region| region.rect = shift.rect(region.rect),
        );
        append(
            &mut list.blockers,
            &old.blockers[from.blockers..end.blockers],
            |rect| *rect = shift.rect(*rect),
        );
        append(
            &mut list.cursors,
            &old.cursors[from.cursors..end.cursors],
            |region| region.rect = shift.rect(region.rect),
        );
        append(
            &mut list.ime_carets,
            &old.ime_carets[from.ime_carets..end.ime_carets],
            |region| region.rect = shift.rect(region.rect),
        );
        let resolved = self.resolved.as_ref();
        let interactions = list.interactions.len();
        append(
            &mut list.interactions,
            &old.interactions[from.interactions..end.interactions],
            |region| {
                region.rect = shift.rect(region.rect);
                region.group = region.group.map(|group| shift.rect(group));
                region.covered_after = slide(region.covered_after, from.regions, to.regions);
            },
        );
        if let Some(resolved) = resolved {
            for (at, region) in list.interactions[interactions..].iter_mut().enumerate() {
                if let Some(interaction) = resolved.get(interactions + at) {
                    region.interaction = *interaction;
                }
            }
        }
        append(
            &mut list.semantics,
            &old.semantics[from.semantics..end.semantics],
            |node| {
                node.rect = shift.rect(node.rect);
                node.parent = node.parent.map(|parent| shift.parent(parent));
            },
        );
        append(
            &mut list.popups,
            &old.popups[from.popups..end.popups],
            |region| region.rect = shift.rect(region.rect),
        );
        append(
            &mut list.pointer_events,
            &old.pointer_events[from.pointer_events..end.pointer_events],
            |region| {
                region.rect = shift.rect(region.rect);
                region.origin = shift.offset(region.origin);
                region.parent = region.parent.map(|parent| shift.pointer_event(parent));
            },
        );
        append(
            &mut list.text_inputs,
            &old.text_inputs[from.text_inputs..end.text_inputs],
            |region| {
                region.rect = shift.rect(region.rect);
                region.origin = shift.offset(region.origin);
            },
        );
    }
}
