use std::cell::Cell;
use std::rc::Rc;

use rustle_geometry::Offset;
use rustle_runtime::{group, is_composing, remember};

use super::{Element, Modifier};
use crate::indication::Indication;
use crate::input::{Focus, Key, KeyCode, KeyEvent, PhysicalKey, PointerButton};
use crate::interaction_source::{
    FocusInteraction, HoverInteraction, Interaction, InteractionId, InteractionSource,
    MutableInteractionSource, PressInteraction,
};
use crate::pointer::{PointerEvent, PointerEventPass, PointerEventType};
use crate::text_input::PlatformTextInputSession;

impl Modifier {
    /// Runs `on_event` for every pointer event of `event_type` over the area, in `pass`,
    /// whether or not something else consumed it: Compose's
    /// `Modifier.onPointerEvent(eventType, pass, onEvent)`.
    ///
    /// The event goes to the topmost area under the pointer that listens this way and to the
    /// ones it was declared inside, in Compose's order: the initial pass from the outermost
    /// in, the main pass from the innermost out, the final pass from the outermost in (see
    /// [`pointer`](crate::pointer)). Positions are relative to the area. While a button is
    /// held, every event goes to the areas the first press landed on, wherever the pointer
    /// is, and enters and exits wait for the release. A consumed scroll does not scroll the
    /// scrolling areas under the pointer.
    pub fn on_pointer_event(
        self,
        event_type: PointerEventType,
        pass: PointerEventPass,
        on_event: impl Fn(&PointerEvent) + 'static,
    ) -> Self {
        self.then(Element::PointerEvent {
            event_type,
            pass,
            on_event: Rc::new(on_event),
        })
    }

    /// Lets the area take text from the platform's input method through `session`, for an
    /// area that draws its own text, such as a terminal or a code editor: Compose's
    /// `PlatformTextInputModifierNode`.
    ///
    /// While a request is started on the session (see
    /// [`PlatformTextInputSession::start_input_method`]) and the area's focus is held (the
    /// nearest [`focusable`](Self::focusable) around this modifier, or else the first inside
    /// it), the input method's composition and committed text go to the request instead of
    /// the key handlers, and the caret the request reports places the input method's
    /// candidate window.
    pub fn platform_text_input(self, session: &PlatformTextInputSession) -> Self {
        self.then(Element::TextInput(session.clone()))
    }

    /// Draws the indication `indication` makes for the interactions of `interaction_source`
    /// over the content: Compose's `Modifier.indication(interactionSource, indication)`.
    /// Nothing is drawn when `indication` is `None`.
    pub fn indication(
        self,
        interaction_source: &InteractionSource,
        indication: Option<&Indication>,
    ) -> Self {
        match indication {
            Some(indication) => self.chain(indication.create(interaction_source)),
            None => self,
        }
    }

    /// Emits a hover into `interaction_source` when the pointer comes over the area, and its
    /// end when it leaves: Compose's `Modifier.hoverable(interactionSource)`.
    pub fn hoverable(self, interaction_source: &MutableInteractionSource) -> Self {
        let entered = interaction_source.clone();
        let left = interaction_source.clone();
        self.on_pointer_event(PointerEventType::Enter, PointerEventPass::Main, move |_| {
            entered.emit(HoverInteraction::Enter {
                id: InteractionId::next(),
            });
        })
        .on_pointer_event(PointerEventType::Exit, PointerEventPass::Main, move |_| {
            left.end_latest(
                |interaction| matches!(interaction, Interaction::Hover(_)),
                |enter| HoverInteraction::Exit { enter }.into(),
            );
        })
    }

    /// Like [`focusable`](Self::focusable), also emitting a focus into `interaction_source`
    /// when the area takes focus and its end when it loses it: Compose's
    /// `Modifier.focusable(interactionSource = ...)`.
    pub fn focusable_with(
        self,
        focus: &Focus,
        interaction_source: &MutableInteractionSource,
        on_key: impl Fn(&crate::KeyEvent) -> bool + 'static,
    ) -> Self {
        let source = interaction_source.clone();
        self.on_focus_changed(move |focused| {
            if focused {
                source.emit(FocusInteraction::Focus {
                    id: InteractionId::next(),
                });
            } else {
                source.end_latest(
                    |interaction| matches!(interaction, Interaction::Focus(_)),
                    |focus| FocusInteraction::Unfocus { focus }.into(),
                );
            }
        })
        .focusable(focus, on_key)
    }

    /// Runs `on_click` when the area is pressed and released with the primary button, or
    /// activated from the keyboard as [`clickable`](Self::clickable) is, and reports its
    /// presses, those of the keys included, and hovers to `interaction_source`, drawn by
    /// `indication`: Compose's `Modifier.clickable(interactionSource, indication, onClick)`.
    ///
    /// The built-in hover and press tint of [`clickable`](Self::clickable) is not drawn: the
    /// indication shows the interactions, and with `None` nothing does. Pass the
    /// [`LOCAL_INDICATION`](crate::LOCAL_INDICATION) for the one the surrounding theme gives.
    /// As in Compose, the pointer keeps the cursor around it rather than showing a hand.
    #[track_caller]
    pub fn clickable_with(
        self,
        interaction_source: &MutableInteractionSource,
        indication: Option<&Indication>,
        on_click: impl Fn() + 'static,
    ) -> Self {
        let pressed = interaction_source.clone();
        let released = interaction_source.clone();
        let on_click: Rc<dyn Fn()> = Rc::new(on_click);
        self.clicked_by_keys(Rc::clone(&on_click), Some(interaction_source.clone()))
            .indication(interaction_source, indication)
            .hoverable(interaction_source)
            .on_pointer_event(
                PointerEventType::Press,
                PointerEventPass::Main,
                move |event| {
                    if event.button == Some(PointerButton::Primary)
                        && let Some(change) = event.changes.first()
                    {
                        pressed.emit(PressInteraction::press(change.position));
                    }
                },
            )
            .on_pointer_event(
                PointerEventType::Release,
                PointerEventPass::Main,
                move |event| {
                    if event.button == Some(PointerButton::Primary) {
                        released.end_latest(
                            |interaction| matches!(interaction, Interaction::Press(_)),
                            |press| PressInteraction::Release { press }.into(),
                        );
                    }
                },
            )
            .then(Element::Clickable(on_click, false, false))
    }

    /// Makes the area the focus target of a clickable, activated by the keys Compose's
    /// `clickable` takes: while it holds focus, one of them going down is taken and reported
    /// to `interaction_source` as a press, and the same key coming up runs `on_click`.
    #[track_caller]
    pub(crate) fn clicked_by_keys(
        self,
        on_click: Rc<dyn Fn()>,
        interaction_source: Option<MutableInteractionSource>,
    ) -> Self {
        let (focus, held) = clickable_focus();
        self.then(Element::Focusable {
            focus,
            on_key: Rc::new(move |event| {
                if !activates_click(event) {
                    return false;
                }
                if event.pressed {
                    if held.get() {
                        return false;
                    }
                    held.set(true);
                    if let Some(source) = &interaction_source {
                        source.emit(PressInteraction::press(Offset::default()));
                    }
                    return true;
                }
                if !held.replace(false) {
                    return false;
                }
                if let Some(source) = &interaction_source {
                    source.end_latest(
                        |interaction| matches!(interaction, Interaction::Press(_)),
                        |press| PressInteraction::Release { press }.into(),
                    );
                }
                on_click();
                true
            }),
            takes_press: false,
            pin: crate::pinnable::current_pinnable_container(),
        })
    }
}

/// The focus of a clickable and whether one of its keys is down, remembered at the call site
/// of the clickable while composing, and new otherwise.
#[track_caller]
fn clickable_focus() -> (Focus, Rc<Cell<bool>>) {
    if !is_composing() {
        return (Focus::new(), Rc::default());
    }
    group(|| {
        let focus = crate::input::remember_focus();
        let held = remember(|| Rc::new(Cell::new(false))).get();
        (focus, held)
    })
}

/// Whether `event` is one of the keys that activate a clickable, going down or up: Enter,
/// the keypad's Enter or the space bar, as Compose's `clickable` takes them.
fn activates_click(event: &KeyEvent) -> bool {
    match &event.key {
        Key::Enter => true,
        Key::Character(text) => text == " ",
        _ => matches!(
            event.physical_key,
            PhysicalKey::Code(KeyCode::NumpadEnter | KeyCode::Space)
        ),
    }
}
