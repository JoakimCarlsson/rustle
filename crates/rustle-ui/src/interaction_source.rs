//! Interactions and the sources that emit them, as Compose's
//! `androidx.compose.foundation.interaction` has them: presses, hovers, focus and drags that
//! components report through a [`MutableInteractionSource`], and that indications and
//! styles read back as state.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustle_geometry::Offset;
use rustle_runtime::{State, mutable_state};

/// Tells one interaction apart from the others, so its end can name its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InteractionId(u64);

impl InteractionId {
    /// An id no other interaction of this thread has.
    pub fn next() -> Self {
        thread_local! {
            static NEXT: Cell<u64> = const { Cell::new(0) };
        }
        NEXT.with(|next| {
            next.set(next.get() + 1);
            InteractionId(next.get())
        })
    }
}

/// A press and its end: Compose's `PressInteraction`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PressInteraction {
    /// The area was pressed at `press_position`, relative to its top-left corner.
    Press {
        /// What the release or cancel names.
        id: InteractionId,
        /// Where it was pressed.
        press_position: Offset,
    },
    /// The press `press` ended with a release.
    Release {
        /// The press that ended.
        press: InteractionId,
    },
    /// The press `press` ended without a click, such as when the pointer left.
    Cancel {
        /// The press that ended.
        press: InteractionId,
    },
}

impl PressInteraction {
    /// A new press at `press_position`.
    pub fn press(press_position: Offset) -> Self {
        Self::Press {
            id: InteractionId::next(),
            press_position,
        }
    }
}

/// The pointer coming over an area and leaving it: Compose's `HoverInteraction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverInteraction {
    /// The pointer came over the area.
    Enter {
        /// What the exit names.
        id: InteractionId,
    },
    /// The pointer that came in with `enter` left.
    Exit {
        /// The enter that ended.
        enter: InteractionId,
    },
}

/// An area taking keyboard focus and losing it: Compose's `FocusInteraction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusInteraction {
    /// The area took focus.
    Focus {
        /// What the unfocus names.
        id: InteractionId,
    },
    /// The area lost the focus it took with `focus`.
    Unfocus {
        /// The focus that ended.
        focus: InteractionId,
    },
}

/// A drag and its end: Compose's `DragInteraction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragInteraction {
    /// A drag started.
    Start {
        /// What the stop or cancel names.
        id: InteractionId,
    },
    /// The drag `start` ended.
    Stop {
        /// The drag that ended.
        start: InteractionId,
    },
    /// The drag `start` was given up.
    Cancel {
        /// The drag that ended.
        start: InteractionId,
    },
}

/// Something that happened to a component: Compose's `Interaction`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Interaction {
    /// A press or its end.
    Press(PressInteraction),
    /// A hover or its end.
    Hover(HoverInteraction),
    /// A focus or its end.
    Focus(FocusInteraction),
    /// A drag or its end.
    Drag(DragInteraction),
}

impl From<PressInteraction> for Interaction {
    /// The press as an interaction.
    fn from(press: PressInteraction) -> Self {
        Self::Press(press)
    }
}

impl From<HoverInteraction> for Interaction {
    /// The hover as an interaction.
    fn from(hover: HoverInteraction) -> Self {
        Self::Hover(hover)
    }
}

impl From<FocusInteraction> for Interaction {
    /// The focus as an interaction.
    fn from(focus: FocusInteraction) -> Self {
        Self::Focus(focus)
    }
}

impl From<DragInteraction> for Interaction {
    /// The drag as an interaction.
    fn from(drag: DragInteraction) -> Self {
        Self::Drag(drag)
    }
}

impl Interaction {
    /// The id of the interaction this one starts, or `None` for one that ends another.
    fn started(&self) -> Option<InteractionId> {
        match *self {
            Self::Press(PressInteraction::Press { id, .. })
            | Self::Hover(HoverInteraction::Enter { id })
            | Self::Focus(FocusInteraction::Focus { id })
            | Self::Drag(DragInteraction::Start { id }) => Some(id),
            _ => None,
        }
    }

    /// The id of the interaction this one ends, or `None` for one that starts.
    fn ended(&self) -> Option<InteractionId> {
        match *self {
            Self::Press(
                PressInteraction::Release { press } | PressInteraction::Cancel { press },
            ) => Some(press),
            Self::Hover(HoverInteraction::Exit { enter }) => Some(enter),
            Self::Focus(FocusInteraction::Unfocus { focus }) => Some(focus),
            Self::Drag(DragInteraction::Stop { start } | DragInteraction::Cancel { start }) => {
                Some(start)
            }
            _ => None,
        }
    }
}

/// What is told of every interaction a source emits.
type InteractionObserver = Rc<dyn Fn(&Interaction)>;

/// What a source holds: the interactions under way and the state read from them.
struct Held {
    /// The interactions started and not yet ended, oldest first.
    active: RefCell<Vec<Interaction>>,
    /// Every interaction emitted, for observers that see each one.
    observers: RefCell<Vec<InteractionObserver>>,
    pressed: State<bool>,
    hovered: State<bool>,
    focused: State<bool>,
    dragged: State<bool>,
}

/// The interactions of a component, read back: Compose's `InteractionSource`.
///
/// Cheap to clone; clones are the same source. Read whether the component is pressed,
/// hovered, focused or dragged with the `collect_is_*_as_state` methods, whose states a
/// composable or a drawing closure reads like any other.
#[derive(Clone)]
pub struct InteractionSource {
    held: Rc<Held>,
}

/// A source components emit their interactions into: Compose's `MutableInteractionSource`.
///
/// Make one with [`MutableInteractionSource::new`] or [`remember_mutable_interaction_source`]
/// and hand it to the components that should share it (`clickable_with`, `hoverable`,
/// `focusable_with`) and to whatever reads it, such as an
/// [`IndicationNodeFactory`](crate::IndicationNodeFactory). It dereferences to the
/// [`InteractionSource`] it fills.
#[derive(Clone)]
pub struct MutableInteractionSource {
    source: InteractionSource,
}

impl InteractionSource {
    /// The interactions started and not yet ended, oldest first.
    pub fn current_interactions(&self) -> Vec<Interaction> {
        self.held.active.borrow().clone()
    }

    /// Whether a press is under way, as state: Compose's `collectIsPressedAsState`.
    pub fn collect_is_pressed_as_state(&self) -> State<bool> {
        self.held.pressed.clone()
    }

    /// Whether the pointer is over the component, as state: Compose's
    /// `collectIsHoveredAsState`.
    pub fn collect_is_hovered_as_state(&self) -> State<bool> {
        self.held.hovered.clone()
    }

    /// Whether the component holds focus, as state: Compose's `collectIsFocusedAsState`.
    pub fn collect_is_focused_as_state(&self) -> State<bool> {
        self.held.focused.clone()
    }

    /// Whether a drag is under way, as state: Compose's `collectIsDraggedAsState`.
    pub fn collect_is_dragged_as_state(&self) -> State<bool> {
        self.held.dragged.clone()
    }

    /// Calls `observer` with every interaction emitted from now on, as collecting Compose's
    /// `interactions` flow does.
    pub fn observe(&self, observer: impl Fn(&Interaction) + 'static) {
        self.held.observers.borrow_mut().push(Rc::new(observer));
    }
}

impl PartialEq for InteractionSource {
    /// Whether both are the same source.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.held, &other.held)
    }
}

impl std::fmt::Debug for InteractionSource {
    /// The interactions under way.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InteractionSource")
            .field("active", &self.held.active.borrow())
            .finish()
    }
}

impl MutableInteractionSource {
    /// A source with no interaction under way: Compose's `MutableInteractionSource()`.
    pub fn new() -> Self {
        Self {
            source: InteractionSource {
                held: Rc::new(Held {
                    active: RefCell::new(Vec::new()),
                    observers: RefCell::new(Vec::new()),
                    pressed: mutable_state(false),
                    hovered: mutable_state(false),
                    focused: mutable_state(false),
                    dragged: mutable_state(false),
                }),
            },
        }
    }

    /// Records `interaction`: one that starts is under way until one that ends it is emitted.
    /// The states read from the source change with it, and observers are told: Compose's
    /// `emit` (and `tryEmit`, which never has to wait here).
    pub fn emit(&self, interaction: impl Into<Interaction>) {
        let interaction = interaction.into();
        let held = &self.source.held;
        {
            let mut active = held.active.borrow_mut();
            if interaction.started().is_some() {
                active.push(interaction);
            } else if let Some(ended) = interaction.ended() {
                active.retain(|under_way| under_way.started() != Some(ended));
            }
        }
        self.refresh();
        let observers = held.observers.borrow().clone();
        for observer in observers {
            observer(&interaction);
        }
    }

    /// The source read back.
    pub fn as_source(&self) -> &InteractionSource {
        &self.source
    }

    /// Ends the latest interaction under way that `matches` picks, with the one `end` makes
    /// from its id. Does nothing when none is under way.
    pub(crate) fn end_latest(
        &self,
        matches: impl Fn(&Interaction) -> bool,
        end: impl FnOnce(InteractionId) -> Interaction,
    ) {
        let latest = self
            .source
            .held
            .active
            .borrow()
            .iter()
            .rev()
            .find(|interaction| matches(interaction))
            .and_then(Interaction::started);
        if let Some(id) = latest {
            self.emit(end(id));
        }
    }

    /// Sets the states read from the source to what is under way, where they changed.
    fn refresh(&self) {
        let held = &self.source.held;
        let active = held.active.borrow().clone();
        let any = |kind: fn(&Interaction) -> bool| active.iter().any(kind);
        let set = |state: &State<bool>, value: bool| {
            if state.get() != value {
                state.set(value);
            }
        };
        set(&held.pressed, any(|i| matches!(i, Interaction::Press(_))));
        set(&held.hovered, any(|i| matches!(i, Interaction::Hover(_))));
        set(&held.focused, any(|i| matches!(i, Interaction::Focus(_))));
        set(&held.dragged, any(|i| matches!(i, Interaction::Drag(_))));
    }
}

impl Default for MutableInteractionSource {
    /// A source with no interaction under way.
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for MutableInteractionSource {
    type Target = InteractionSource;

    /// The source read back.
    fn deref(&self) -> &InteractionSource {
        &self.source
    }
}

impl PartialEq for MutableInteractionSource {
    /// Whether both are the same source.
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl std::fmt::Debug for MutableInteractionSource {
    /// The interactions under way.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}

/// A source remembered across recompositions: Compose's
/// `remember { MutableInteractionSource() }`.
#[track_caller]
pub fn remember_mutable_interaction_source() -> MutableInteractionSource {
    rustle_runtime::remember(MutableInteractionSource::new).get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_follow_what_is_under_way() {
        let source = MutableInteractionSource::new();
        let pressed = source.collect_is_pressed_as_state();
        let press = PressInteraction::press(Offset::new(1.0, 2.0));
        source.emit(press);
        assert!(pressed.get());
        let PressInteraction::Press { id, .. } = press else {
            unreachable!()
        };
        source.emit(PressInteraction::Release { press: id });
        assert!(!pressed.get());
        assert!(source.current_interactions().is_empty());
    }

    #[test]
    fn each_kind_has_its_own_state_and_observers_see_every_interaction() {
        let source = MutableInteractionSource::new();
        let seen = Rc::new(Cell::new(0));
        let counted = Rc::clone(&seen);
        source.observe(move |_| counted.set(counted.get() + 1));
        let enter = InteractionId::next();
        let focus = InteractionId::next();
        source.emit(HoverInteraction::Enter { id: enter });
        source.emit(FocusInteraction::Focus { id: focus });
        assert!(source.collect_is_hovered_as_state().get());
        assert!(source.collect_is_focused_as_state().get());
        assert!(!source.collect_is_dragged_as_state().get());
        source.end_latest(
            |interaction| matches!(interaction, Interaction::Hover(_)),
            |enter| HoverInteraction::Exit { enter }.into(),
        );
        assert!(!source.collect_is_hovered_as_state().get());
        assert!(source.collect_is_focused_as_state().get());
        assert_eq!(seen.get(), 3);
    }
}
