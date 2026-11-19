use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::DrawCommand;

use crate::cursor::CursorIcon;
use crate::gesture::PointerInputHandler;
use crate::input::{DropHandler, Focus, KeyHandler, PointerButton, PointerEvent, PressEvent};
use crate::interaction::{Interaction, PointerState, resolve};
use crate::scroll::ScrollState;
use crate::semantics::Semantics;

/// The result of laying out a tree: what to draw, what can be clicked and what scrolls.
#[derive(Default)]
pub struct DisplayList {
    /// Drawing commands in paint order.
    pub commands: Vec<DrawCommand>,
    /// Clickable areas in paint order, so later ones are on top.
    pub regions: Vec<HitRegion>,
    /// Scrolling areas in paint order, so later ones are inside earlier ones.
    pub scrolls: Vec<ScrollRegion>,
    /// Areas that take keyboard focus, in paint order, which is also tab order.
    pub focusables: Vec<FocusRegion>,
    /// Areas named for tests, in paint order. See [`Modifier::test_tag`](crate::Modifier::test_tag).
    pub test_tags: Vec<TaggedNode>,
    /// Areas that react to a press, in paint order.
    pub presses: Vec<PressRegion>,
    /// Areas that react to a pointer button other than (or as well as) the primary one, in paint order.
    pub buttons: Vec<ButtonRegion>,
    /// Areas that see key events without holding focus, in paint order (outermost first).
    pub key_handlers: Vec<KeyRegion>,
    /// Where the caret of the focused text field is, in the window, for placing an input
    /// method's candidate popup. `None` when no text field is being edited.
    pub ime_area: Option<Rect>,
    /// Areas that follow a drag, in paint order.
    pub drags: Vec<DragRegion>,
    /// Areas that want to know when the pointer is over them, in paint order.
    pub hovers: Vec<HoverRegion>,
    /// Areas that react to a long press, in paint order.
    pub long_presses: Vec<LongPressRegion>,
    /// Areas that follow the pointer over them, in paint order.
    pub pointer_moves: Vec<PointerMoveRegion>,
    /// Areas that can be picked up and dragged elsewhere, in paint order.
    pub drag_sources: Vec<DragSourceRegion>,
    /// Areas something dragged can be dropped on, in paint order.
    pub drop_targets: Vec<DropTargetRegion>,
    /// Areas a drag and drop starts from, in paint order. See
    /// [`Modifier::drag_and_drop_source`](crate::Modifier::drag_and_drop_source).
    pub drag_and_drop_sources: Vec<crate::DragAndDropSourceRegion>,
    /// Areas that take part in drags and drops, in paint order. See
    /// [`Modifier::drag_and_drop_target`](crate::Modifier::drag_and_drop_target).
    pub drag_and_drop_targets: Vec<crate::DragAndDropTargetRegion>,
    /// Areas files can be dropped on, in paint order.
    pub file_drops: Vec<FileDropRegion>,
    /// Areas that see the raw pointer stream, in paint order.
    pub pointer_inputs: Vec<PointerInputRegion>,
    /// Areas that react to a pinch gesture, in paint order.
    pub pinches: Vec<PinchRegion>,
    /// Areas that count as used by the interface wherever the pointer is over them, in paint
    /// order. See [`Modifier::blocks_pointer`](crate::Modifier::blocks_pointer).
    pub blockers: Vec<Rect>,
    /// Areas that choose the mouse cursor, in paint order, so later ones are on top.
    pub cursors: Vec<CursorRegion>,
    /// Where areas that draw their own text take text input, in paint order. See
    /// [`Modifier::ime_caret`](crate::Modifier::ime_caret).
    pub ime_carets: Vec<ImeCaretRegion>,
    /// Areas drawn according to their interaction state, in paint order. See
    /// [`Modifier::draw_behind_interactive`](crate::Modifier::draw_behind_interactive).
    pub interactions: Vec<InteractionRegion>,
    /// What the areas mean, in paint order, for assistive technology.
    pub semantics: Vec<SemanticsNode>,
    /// The popups shown, in paint order, so later ones are on top. See
    /// [`Modifier::popup_placement`](crate::Modifier::popup_placement).
    pub popups: Vec<PopupRegion>,
    /// Areas that hear pointer events in passes, in paint order. See
    /// [`Modifier::on_pointer_event`](crate::Modifier::on_pointer_event).
    pub pointer_events: Vec<crate::pointer::PointerEventRegion>,
    /// Areas that take text from the input method through a session, in paint order. See
    /// [`Modifier::platform_text_input`](crate::Modifier::platform_text_input).
    pub text_inputs: Vec<crate::text_input::TextInputRegion>,
}

/// A popup shown in the window: where it was painted and how it is dismissed.
pub struct PopupRegion {
    /// Where the popup was painted, in the window.
    pub rect: Rect,
    /// How the popup behaves.
    pub properties: crate::PopupProperties,
    /// What to call when the popup should be dismissed, if anything.
    pub on_dismiss_request: Option<Rc<dyn Fn()>>,
}

impl PopupRegion {
    /// Whether a press at `point` is outside the popup and asks to dismiss it.
    pub fn dismissed_by_press_at(&self, point: Offset) -> bool {
        self.properties.dismiss_on_click_outside
            && self.on_dismiss_request.is_some()
            && !contains(self.rect, point)
    }
}

/// An area drawn according to its interaction state, cut to what is visible, with what
/// decides that state and the state it was painted in.
pub struct InteractionRegion {
    /// The part of the area that is visible, in the window.
    pub rect: Rect,
    /// The focus whose holder counts as focused, unless `focused` is given.
    pub focus: Option<Focus>,
    /// Says whether the area counts as focused, given from outside with
    /// [`Modifier::interaction_focused`](crate::Modifier::interaction_focused) or
    /// [`Modifier::interaction_focused_with`](crate::Modifier::interaction_focused_with); it
    /// wins over `focus`.
    pub focused: Option<Rc<dyn Fn() -> bool>>,
    /// The visible rectangle of the interaction group around the area, given with
    /// [`Modifier::interaction_group`](crate::Modifier::interaction_group).
    pub group: Option<Rect>,
    /// How many clickable regions were painted when this area and what is inside it had been.
    pub covered_after: usize,
    /// The state the area was painted in.
    pub interaction: Interaction,
}

/// Where an area that draws its own text takes text input, cut to what is visible, and the
/// focus that must be held for it to count.
pub struct ImeCaretRegion {
    pub rect: Rect,
    /// The focus of the nearest focusable around the area, or the first inside it; `None`
    /// when there is neither, and then the region never counts.
    pub focus: Option<Focus>,
}

/// An area that chooses the mouse cursor, cut to what is visible.
pub struct CursorRegion {
    pub rect: Rect,
    pub icon: CursorIcon,
}

/// What an area means, where it is, and what activating it does.
pub struct SemanticsNode {
    pub rect: Rect,
    pub semantics: Semantics,
    /// The index in [`DisplayList::semantics`] of the area this one is described inside,
    /// or `None` for a child of the window. Parents come before their children.
    pub parent: Option<usize>,
    /// Runs when assistive technology activates the area.
    pub on_click: Option<Rc<dyn Fn()>>,
}

/// An area that can hold keyboard focus, cut to what is visible.
pub struct FocusRegion {
    pub rect: Rect,
    pub focus: Focus,
    pub on_key: KeyHandler,
    /// Whether a press of the primary button on it gives it focus: true for
    /// [`Modifier::focusable`](crate::Modifier::focusable), false for
    /// [`Modifier::focus_target`](crate::Modifier::focus_target).
    pub takes_press: bool,
    /// What [`Modifier::on_focus_changed`](crate::Modifier::on_focus_changed) asked to be
    /// told when its focus comes or goes.
    pub on_focus_changed: Vec<Rc<dyn Fn(bool)>>,
}

/// An area named for tests with [`Modifier::test_tag`](crate::Modifier::test_tag): Compose's
/// semantics node found by `onNodeWithTag`.
#[derive(Clone, Debug, PartialEq)]
pub struct TaggedNode {
    /// The tag it was given.
    pub tag: Rc<str>,
    /// Where it was laid out in the window, whether or not it can be seen: Compose's
    /// `boundsInRoot`.
    pub bounds: Rect,
    /// The part of it the clips around it leave visible; empty when it is scrolled or
    /// clipped out of sight.
    pub visible: Rect,
    /// The area that takes focus for it, as an index into [`DisplayList::focusables`]: the
    /// first focusable painted inside it, or else the one around it.
    pub focusable: Option<usize>,
}

impl TaggedNode {
    /// Whether any of it can be seen: Compose's `assertIsDisplayed`.
    pub fn is_displayed(&self) -> bool {
        self.visible.size.width > 0.0 && self.visible.size.height > 0.0
    }

    /// The middle of its visible part, where a test presses it, or of its bounds when none
    /// of it can be seen.
    pub fn center(&self) -> Offset {
        let rect = if self.is_displayed() {
            self.visible
        } else {
            self.bounds
        };
        Offset::new(
            rect.origin.x + rect.size.width / 2.0,
            rect.origin.y + rect.size.height / 2.0,
        )
    }
}

/// An area that reacts to a press, cut to what is visible.
pub struct PressRegion {
    pub rect: Rect,
    /// Where the area's top-left corner is, before it was cut to what is visible.
    pub origin: Offset,
    pub on_press: Rc<dyn Fn(PressEvent)>,
    /// The view configuration in effect where the area was made, which presses on it are
    /// counted by.
    pub view_configuration: crate::ViewConfiguration,
}

/// An area that reacts to one pointer button, cut to what is visible.
pub struct ButtonRegion {
    pub rect: Rect,
    /// Where the area's top-left corner is, before it was cut to what is visible.
    pub origin: Offset,
    /// The button this reacts to.
    pub button: PointerButton,
    pub on_event: Rc<dyn Fn(&PointerEvent)>,
}

/// An area that sees key events that happen inside it, cut to what is visible.
pub struct KeyRegion {
    pub rect: Rect,
    /// The focusable areas painted inside this one, as indices into [`DisplayList::focusables`].
    /// A key event for one of them reaches this handler too.
    pub focusables: std::ops::Range<usize>,
    /// The focusable areas of content raised with a z-index that was declared inside this
    /// one and painted later, over the rest, such as a popup: their keys reach this handler
    /// as well, as if they were painted in place.
    pub raised_focusables: Vec<std::ops::Range<usize>>,
    /// Whether this sees the event before the focused area (a preview) instead of after it.
    pub preview: bool,
    pub on_key: KeyHandler,
}

/// An area that can be picked up, cut to what is visible.
pub struct DragSourceRegion {
    pub rect: Rect,
    /// The button that picks the area up.
    pub button: PointerButton,
    pub payload: Rc<dyn std::any::Any>,
    /// The drawing commands of the area, as indices into [`DisplayList::commands`],
    /// so what is being dragged can be drawn again under the pointer.
    pub commands: std::ops::Range<usize>,
    /// How far the pointer moves with the button held before the area is picked up.
    pub touch_slop: f32,
}

/// An area something dragged can be dropped on, cut to what is visible.
pub struct DropTargetRegion {
    pub rect: Rect,
    pub on_drop: DropHandler,
}

/// An area files can be dropped on, cut to what is visible.
pub struct FileDropRegion {
    pub rect: Rect,
    pub on_drop: Rc<dyn Fn(std::path::PathBuf)>,
}

/// An area that sees the raw pointer stream, cut to what is visible.
pub struct PointerInputRegion {
    pub rect: Rect,
    /// Where the area's top-left corner is, before it was cut to what is visible.
    pub origin: Offset,
    pub on_event: PointerInputHandler,
}

/// An area that reacts to a pinch gesture, cut to what is visible.
pub struct PinchRegion {
    pub rect: Rect,
    pub on_pinch: Rc<dyn Fn(f32)>,
}

/// An area that follows the pointer over it, cut to what is visible.
pub struct PointerMoveRegion {
    pub rect: Rect,
    /// Where the area's top-left corner is, before it was cut to what is visible.
    pub origin: Offset,
    pub on_move: Rc<dyn Fn(Option<Offset>)>,
    /// How many clickable regions were painted when this area and what is inside it had been.
    pub covered_after: usize,
}

/// An area that reacts to a long press, cut to what is visible.
pub struct LongPressRegion {
    pub rect: Rect,
    pub on_long_press: Rc<dyn Fn()>,
    /// The view configuration in effect where the area was made: how long the primary
    /// button is held before `on_long_press` runs, and how far it may move meanwhile.
    pub view_configuration: crate::ViewConfiguration,
}

/// An area that wants to know when the pointer is over it, cut to what is visible.
pub struct HoverRegion {
    pub rect: Rect,
    pub on_hover: Rc<dyn Fn(bool)>,
    /// How many clickable regions were painted when this area and what is inside it had been.
    pub covered_after: usize,
}

/// An area that follows a drag, cut to what is visible.
pub struct DragRegion {
    /// What to run when this drag ends, if the area said.
    pub on_end: Option<Rc<dyn Fn()>>,
    pub rect: Rect,
    /// Where the area's top-left corner is, before it was cut to what is visible.
    pub origin: Offset,
    pub on_drag: Rc<dyn Fn(Offset)>,
    /// Whether a press here belongs to this drag alone, as one on a scrollbar does: the
    /// areas beneath are not pressed, focused or clicked.
    pub takes_pointer: bool,
}

/// An area that runs a handler when clicked, cut to what is visible.
pub struct HitRegion {
    pub rect: Rect,
    /// The corner radius of the background the region sits on, for drawing feedback.
    pub corner_radius: f32,
    /// Whether the runner tints the region while hovered or pressed.
    pub feedback: bool,
    pub on_click: Rc<dyn Fn()>,
}

/// An area that scrolls, cut to what is visible.
pub struct ScrollRegion {
    pub rect: Rect,
    pub state: ScrollState,
    pub vertical: bool,
}

pub(crate) fn contains(rect: Rect, point: Offset) -> bool {
    let Rect { origin, size } = rect;
    point.x >= origin.x
        && point.x < origin.x + size.width
        && point.y >= origin.y
        && point.y < origin.y + size.height
}

pub(crate) fn intersect(a: Rect, b: Rect) -> Rect {
    let left = a.origin.x.max(b.origin.x);
    let top = a.origin.y.max(b.origin.y);
    let right = (a.origin.x + a.size.width).min(b.origin.x + b.size.width);
    let bottom = (a.origin.y + a.size.height).min(b.origin.y + b.size.height);
    Rect::new(
        Offset::new(left, top),
        Size::new((right - left).max(0.0), (bottom - top).max(0.0)),
    )
}

impl DisplayList {
    /// Where text input goes now, in the window, for placing an input method's candidate
    /// popup: the caret of the text being edited ([`ime_area`](Self::ime_area)), or else the
    /// caret the started request of the focused
    /// [`platform_text_input`](crate::Modifier::platform_text_input) area reports, or else the
    /// last [`ime_caret`](crate::Modifier::ime_caret) area whose focus is held. `None` when
    /// none applies.
    pub fn focused_ime_area(&self) -> Option<Rect> {
        self.ime_area
            .or_else(|| self.focused_text_input().and_then(|region| region.caret()))
            .or_else(|| {
                self.ime_carets
                    .iter()
                    .rev()
                    .find(|caret| caret.focus.as_ref().is_some_and(Focus::is_focused))
                    .map(|caret| caret.rect)
            })
    }

    /// Whether any area that reacts to the pointer or blocks it contains `point`: clickable,
    /// pressable, draggable, scrollable, focusable, hoverable, drawn by its interaction state,
    /// a drag source, a drop target, one that listens to a pointer button, or one marked
    /// [`blocks_pointer`](crate::Modifier::blocks_pointer). A host that shares the window
    /// with the interface uses it to tell whether the pointer is over the interface or over
    /// what lies beneath.
    pub fn interactive_at(&self, point: Offset) -> bool {
        let over = |rect: Rect| contains(rect, point);
        self.blocks_pointer_at(point)
            || self.regions.iter().any(|region| over(region.rect))
            || self.scrolls.iter().any(|region| over(region.rect))
            || self.focusables.iter().any(|region| over(region.rect))
            || self.presses.iter().any(|region| over(region.rect))
            || self.buttons.iter().any(|region| over(region.rect))
            || self.drags.iter().any(|region| over(region.rect))
            || self.hovers.iter().any(|region| over(region.rect))
            || self.interactions.iter().any(|region| over(region.rect))
            || self.long_presses.iter().any(|region| over(region.rect))
            || self.pointer_moves.iter().any(|region| over(region.rect))
            || self.drag_sources.iter().any(|region| over(region.rect))
            || self.drop_targets.iter().any(|region| over(region.rect))
            || self.pointer_inputs.iter().any(|region| over(region.rect))
            || self.pointer_events.iter().any(|region| over(region.rect))
    }

    /// Whether an area marked [`blocks_pointer`](crate::Modifier::blocks_pointer) contains `point`.
    pub fn blocks_pointer_at(&self, point: Offset) -> bool {
        self.blockers.iter().any(|rect| contains(*rect, point))
    }

    /// The topmost clickable region containing `point`.
    pub fn hit_test(&self, point: Offset) -> Option<&HitRegion> {
        self.regions
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost focusable region containing `point`.
    pub fn focus_at(&self, point: Offset) -> Option<&FocusRegion> {
        self.focusables
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost draggable region containing `point`.
    pub fn drag_source_at(
        &self,
        point: Offset,
        button: PointerButton,
    ) -> Option<&DragSourceRegion> {
        self.drag_sources
            .iter()
            .rev()
            .find(|region| region.button == button && contains(region.rect, point))
    }

    /// The pointer-input regions containing `point`, topmost first.
    pub fn pointer_inputs_at(&self, point: Offset) -> impl Iterator<Item = &PointerInputRegion> {
        self.pointer_inputs
            .iter()
            .rev()
            .filter(move |region| contains(region.rect, point))
    }

    /// What to call for a press at `point`: the dismiss requests of the popups it is outside
    /// of that are dismissed by a press outside, topmost first.
    pub fn popups_dismissed_by_press_at(&self, point: Offset) -> Vec<Rc<dyn Fn()>> {
        self.popups
            .iter()
            .rev()
            .filter(|popup| popup.dismissed_by_press_at(point))
            .filter_map(|popup| popup.on_dismiss_request.clone())
            .collect()
    }

    /// What Escape dismisses: the topmost focusable popup dismissed by Escape, if any.
    pub fn popup_dismissed_by_escape(&self) -> Option<Rc<dyn Fn()>> {
        self.popups
            .iter()
            .rev()
            .find(|popup| popup.properties.focusable)
            .filter(|popup| popup.properties.dismiss_on_back_press)
            .and_then(|popup| popup.on_dismiss_request.clone())
    }

    /// The topmost drag-and-drop source containing `point`.
    pub fn drag_and_drop_source_at(
        &self,
        point: Offset,
    ) -> Option<&crate::DragAndDropSourceRegion> {
        self.drag_and_drop_sources
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost drop target containing `point`.
    pub fn drop_target_at(&self, point: Offset) -> Option<&DropTargetRegion> {
        self.drop_targets
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost file-drop region containing `point`.
    pub fn file_drop_at(&self, point: Offset) -> Option<&FileDropRegion> {
        self.file_drops
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost pinch region containing `point`.
    pub fn pinch_at(&self, point: Offset) -> Option<&PinchRegion> {
        self.pinches
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost long-press region containing `point`.
    pub fn long_press_at(&self, point: Offset) -> Option<&LongPressRegion> {
        self.long_presses
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The interaction state each of [`interactions`](Self::interactions) is in for
    /// `pointer` and the focus held now, in the same order.
    pub fn resolve_interactions(&self, pointer: PointerState) -> Vec<Interaction> {
        self.interactions
            .iter()
            .map(|region| self.resolve_interaction(region, pointer))
            .collect()
    }

    /// Whether any of [`interactions`](Self::interactions) would be painted differently for
    /// `pointer` and the focus held now than it was, so the list should be painted again.
    pub fn interactions_changed(&self, pointer: PointerState) -> bool {
        self.interactions
            .iter()
            .any(|region| self.resolve_interaction(region, pointer) != region.interaction)
    }

    /// The interaction state `region` is in for `pointer` and the focus held now.
    fn resolve_interaction(
        &self,
        region: &InteractionRegion,
        pointer: PointerState,
    ) -> Interaction {
        let covered = pointer
            .position
            .is_some_and(|point| self.covered_at(region.covered_after, point));
        resolve(
            region.rect,
            region.focus.as_ref(),
            region.focused.as_ref().map(|focused| focused()),
            region.group,
            pointer,
            covered,
        )
    }

    /// Every hover region containing `point` that nothing painted later covers.
    pub fn hovers_at(&self, point: Offset) -> impl Iterator<Item = &HoverRegion> {
        self.hovers.iter().filter(move |region| {
            contains(region.rect, point) && !self.covered_at(region.covered_after, point)
        })
    }

    /// Every pointer-follow region containing `point` that nothing painted later covers.
    pub fn pointer_moves_at(&self, point: Offset) -> impl Iterator<Item = &PointerMoveRegion> {
        self.pointer_moves.iter().filter(move |region| {
            contains(region.rect, point) && !self.covered_at(region.covered_after, point)
        })
    }

    /// Whether a clickable region painted after the first `painted` ones contains `point`,
    /// such as a dialog over the page, which keeps the pointer from reaching what is beneath.
    fn covered_at(&self, painted: usize, point: Offset) -> bool {
        self.regions
            .iter()
            .skip(painted)
            .any(|region| contains(region.rect, point))
    }

    /// The cursor the topmost cursor region containing `point` asks for, if any.
    pub fn cursor_at(&self, point: Offset) -> Option<CursorIcon> {
        self.cursors
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
            .map(|region| region.icon)
    }

    /// The topmost drag region containing `point`.
    pub fn drag_at(&self, point: Offset) -> Option<&DragRegion> {
        self.drags
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost region reacting to `button` that contains `point`.
    pub fn button_at(&self, point: Offset, button: PointerButton) -> Option<&ButtonRegion> {
        self.buttons
            .iter()
            .rev()
            .find(|region| region.button == button && contains(region.rect, point))
    }

    /// Gives `event` to the handlers it goes through, stopping at the first that handles it,
    /// and returns whether one did. The order is: [`Modifier::on_preview_key_event`](crate::Modifier::on_preview_key_event)
    /// areas around the focused area, outermost first; the focused area; then
    /// [`Modifier::on_key_event`](crate::Modifier::on_key_event) areas around it, innermost first.
    /// With no focused area, every such area in the window is "around" it.
    pub fn dispatch_key(&self, focused: Option<&Focus>, event: &crate::KeyEvent) -> bool {
        let index = focused.and_then(|focus| {
            self.focusables
                .iter()
                .position(|region| region.focus.id() == focus.id())
        });
        // Handlers are cloned out first: running one may change state, not this list.
        let around: Vec<&KeyRegion> = self
            .key_handlers
            .iter()
            .filter(|region| {
                index.is_none_or(|index| {
                    region.focusables.contains(&index)
                        || region
                            .raised_focusables
                            .iter()
                            .any(|raised| raised.contains(&index))
                })
            })
            .collect();
        let preview = around.iter().filter(|region| region.preview);
        let bubble = around.iter().rev().filter(|region| !region.preview);
        let focused = index.map(|index| &self.focusables[index].on_key);
        let handlers: Vec<KeyHandler> = preview
            .map(|region| Rc::clone(&region.on_key))
            .chain(focused.map(Rc::clone))
            .chain(bubble.map(|region| Rc::clone(&region.on_key)))
            .collect();
        handlers.iter().any(|handler| handler(event))
    }

    /// The areas named `tag` for tests, in paint order: Compose's `onAllNodesWithTag`.
    pub fn nodes_with_tag(&self, tag: &str) -> Vec<TaggedNode> {
        self.test_tags
            .iter()
            .filter(|node| &*node.tag == tag)
            .cloned()
            .collect()
    }

    /// The first area named `tag` for tests in paint order, if any: Compose's `onNodeWithTag`.
    pub fn node_with_tag(&self, tag: &str) -> Option<TaggedNode> {
        self.test_tags
            .iter()
            .find(|node| &*node.tag == tag)
            .cloned()
    }

    /// The focus of the area named `tag` for tests (see [`TaggedNode::focusable`]), for a test
    /// that requests focus on it: Compose's `onNodeWithTag(tag).requestFocus()`.
    pub fn focus_with_tag(&self, tag: &str) -> Option<Focus> {
        let index = self.node_with_tag(tag)?.focusable?;
        Some(self.focusables.get(index)?.focus.clone())
    }

    /// The focusable region holding `focus`, if it was painted.
    pub fn focus_region(&self, focus: &Focus) -> Option<&FocusRegion> {
        self.focusables
            .iter()
            .find(|region| region.focus.id() == focus.id())
    }

    /// The topmost press region containing `point`.
    pub fn press_at(&self, point: Offset) -> Option<&PressRegion> {
        self.presses
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The innermost region containing `point` that scrolls along the given axis.
    pub fn scroll_at(&self, point: Offset, vertical: bool) -> Option<&ScrollRegion> {
        self.scrolls
            .iter()
            .rev()
            .find(|region| region.vertical == vertical && contains(region.rect, point))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use rustle_text::{StyledSpan, TextMeasurer, TextStyle};
    use rustle_unit::Constraints;

    use super::*;
    use crate::{
        Key, KeyEvent, Modifier, Modifiers, Overlay, PointerButton, canvas, composition,
        display_list, layout, remember_focus,
    };

    struct NoText;

    impl TextMeasurer for NoText {
        fn measure(&mut self, _: &str, _: &TextStyle, _: f32) -> Size {
            Size::new(0.0, 0.0)
        }
        fn measure_spans(&mut self, _: &[StyledSpan], _: &TextStyle, _: f32) -> Size {
            Size::new(0.0, 0.0)
        }
        fn layout(&mut self, _: &[StyledSpan], _: &TextStyle, _: f32) -> rustle_text::TextLayout {
            unimplemented!("the tests lay out no text")
        }
    }

    type Log = Rc<RefCell<Vec<&'static str>>>;

    fn recorder(
        log: &Log,
        name: &'static str,
        handled: bool,
    ) -> impl Fn(&KeyEvent) -> bool + use<> {
        let log = Rc::clone(log);
        move |_| {
            log.borrow_mut().push(name);
            handled
        }
    }

    /// outer(preview, bubble) > inner(bubble) > field a; outer > field b.
    fn keyed(log: &Log, stop_at: &'static str) -> (DisplayList, Vec<Focus>) {
        let foci = Rc::new(RefCell::new(Vec::new()));
        let root = {
            let (log, foci) = (Rc::clone(log), Rc::clone(&foci));
            move || {
                let (a, b) = (remember_focus(), remember_focus());
                foci.borrow_mut().clear();
                foci.borrow_mut().extend([a.clone(), b.clone()]);
                let outer = Modifier::new()
                    .size(100.0, 100.0)
                    .on_preview_key_event(recorder(&log, "preview", stop_at == "preview"))
                    .on_key_event(recorder(&log, "outer", stop_at == "outer"));
                layout(outer, Overlay::default(), || {
                    let inner = Modifier::new().size(40.0, 40.0).on_key_event(recorder(
                        &log,
                        "inner",
                        stop_at == "inner",
                    ));
                    layout(inner, Overlay::default(), || {
                        canvas(
                            Modifier::new()
                                .size(10.0, 10.0)
                                .focusable(&a, recorder(&log, "a", stop_at == "a")),
                            |_| {},
                        );
                    });
                    canvas(
                        Modifier::new()
                            .size(10.0, 10.0)
                            .focusable(&b, recorder(&log, "b", false)),
                        |_| {},
                    );
                });
            }
        };
        let mut composition = composition(root);
        composition.compose();
        let tree = composition.output().expect("composed");
        let display = display_list(
            tree,
            Constraints::loose(Size::new(200.0, 200.0)),
            &mut NoText,
        );
        let foci = foci.borrow().clone();
        (display, foci)
    }

    #[test]
    fn keys_bubble_from_the_focused_area_outwards() {
        let log = Log::default();
        let (display, foci) = keyed(&log, "none");
        let event = KeyEvent::press(Key::F(5), Modifiers::NONE);
        assert!(!display.dispatch_key(Some(&foci[0]), &event));
        assert_eq!(*log.borrow(), ["preview", "a", "inner", "outer"]);
        log.borrow_mut().clear();
        // Field b is not inside `inner`.
        assert!(!display.dispatch_key(Some(&foci[1]), &event));
        assert_eq!(*log.borrow(), ["preview", "b", "outer"]);
    }

    #[test]
    fn a_handler_stops_the_event() {
        for (stop, seen) in [
            ("preview", vec!["preview"]),
            ("a", vec!["preview", "a"]),
            ("inner", vec!["preview", "a", "inner"]),
        ] {
            let log = Log::default();
            let (display, foci) = keyed(&log, stop);
            let event = KeyEvent::press(Key::Escape, Modifiers::NONE);
            assert!(display.dispatch_key(Some(&foci[0]), &event));
            assert_eq!(*log.borrow(), seen);
        }
    }

    #[test]
    fn with_no_focus_every_handler_sees_the_key_innermost_first() {
        let log = Log::default();
        let (display, _) = keyed(&log, "none");
        let event = KeyEvent::press(Key::F(1), Modifiers::CTRL);
        assert!(!display.dispatch_key(None, &event));
        assert_eq!(*log.borrow(), ["preview", "inner", "outer"]);
    }

    #[test]
    fn button_regions_are_found_by_button() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let root = {
            let seen = Rc::clone(&seen);
            move || {
                let secondary = Rc::clone(&seen);
                let middle = Rc::clone(&seen);
                canvas(
                    Modifier::new()
                        .size(50.0, 50.0)
                        .on_secondary_click(move |event| secondary.borrow_mut().push(event.button))
                        .on_middle_click(move |event| middle.borrow_mut().push(event.button)),
                    |_| {},
                );
            }
        };
        let mut composition = composition(root);
        composition.compose();
        let tree = composition.output().expect("composed");
        let display = display_list(
            tree,
            Constraints::loose(Size::new(200.0, 200.0)),
            &mut NoText,
        );
        let at = Offset::new(10.0, 10.0);
        assert!(display.button_at(at, PointerButton::Primary).is_none());
        assert!(
            display
                .button_at(Offset::new(80.0, 10.0), PointerButton::Middle)
                .is_none()
        );
        for button in [PointerButton::Secondary, PointerButton::Middle] {
            let region = display.button_at(at, button).expect("region");
            (region.on_event)(&crate::PointerEvent {
                button,
                phase: crate::PointerPhase::Click,
                position: at,
                window_position: at,
                modifiers: Modifiers::NONE,
                click_count: 1,
            });
            // A press is not a click.
            (region.on_event)(&crate::PointerEvent {
                phase: crate::PointerPhase::Press,
                button,
                position: at,
                window_position: at,
                modifiers: Modifiers::NONE,
                click_count: 1,
            });
        }
        assert_eq!(
            *seen.borrow(),
            [PointerButton::Secondary, PointerButton::Middle]
        );
    }

    #[test]
    fn an_area_that_blocks_the_pointer_counts_as_used_where_nothing_reacts() {
        let mut composition = composition(|| {
            layout(
                Modifier::new().size(100.0, 100.0),
                Overlay::default(),
                || {
                    canvas(Modifier::new().size(40.0, 40.0), |_| {});
                    layout(
                        Modifier::new()
                            .offset(50.0, 0.0)
                            .size(40.0, 40.0)
                            .blocks_pointer(),
                        Overlay::default(),
                        || {
                            canvas(Modifier::new().size(10.0, 10.0).clickable(|| {}), |_| {});
                        },
                    );
                },
            );
        });
        composition.compose();
        let tree = composition.output().expect("composed");
        let display = display_list(
            tree,
            Constraints::loose(Size::new(200.0, 200.0)),
            &mut NoText,
        );
        let first = Offset::new(10.0, 10.0);
        let panel_empty_space = Offset::new(80.0, 30.0);
        let outside = Offset::new(95.0, 90.0);
        assert!(!display.interactive_at(first), "plain content is not used");
        assert!(display.interactive_at(panel_empty_space));
        assert!(display.blocks_pointer_at(panel_empty_space));
        assert!(!display.interactive_at(outside));
    }

    /// A popup that takes presses keeps presses and drags that land on it from the areas
    /// beneath, while those inside it still get theirs.
    #[test]
    fn a_popup_takes_the_presses_and_drags_that_land_on_it() {
        let mut composition = composition(|| {
            layout(
                Modifier::new()
                    .size(100.0, 100.0)
                    .on_press(|_| {})
                    .on_drag(|_| {}),
                Overlay::default(),
                || {
                    layout(
                        Modifier::new().popup_placement(
                            crate::AlignmentOffsetPositionProvider {
                                alignment: crate::BoxAlignment::TopStart,
                                offset: Offset::new(20.0, 20.0),
                            },
                            crate::PopupProperties::default(),
                            Some(Rc::new(|| {})),
                        ),
                        Overlay::default(),
                        || {
                            canvas(Modifier::new().size(40.0, 40.0), |_| {});
                            canvas(Modifier::new().size(10.0, 10.0).on_press(|_| {}), |_| {});
                        },
                    );
                },
            );
        });
        composition.compose();
        let tree = composition.output().expect("composed");
        let display = display_list(
            tree,
            Constraints::loose(Size::new(200.0, 200.0)),
            &mut NoText,
        );
        let beneath = Offset::new(5.0, 5.0);
        let on_popup = Offset::new(50.0, 50.0);
        let inside = Offset::new(25.0, 25.0);
        let size = |region: Option<&PressRegion>| region.map(|region| region.rect.size);
        assert_eq!(
            size(display.press_at(beneath)),
            Some(Size::new(100.0, 100.0))
        );
        assert_eq!(
            size(display.press_at(on_popup)),
            Some(Size::new(40.0, 40.0))
        );
        assert_eq!(size(display.press_at(inside)), Some(Size::new(10.0, 10.0)));
        assert_eq!(
            display.drag_at(beneath).map(|region| region.rect.size),
            Some(Size::new(100.0, 100.0))
        );
        assert_eq!(
            display.drag_at(on_popup).map(|region| region.rect.size),
            Some(Size::new(40.0, 40.0))
        );
    }
}
