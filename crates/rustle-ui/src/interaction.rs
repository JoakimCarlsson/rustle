//! Interaction state resolved when painting: whether the pointer is over an area, presses it,
//! and whether the area has keyboard focus, so styles that follow them need no composition.

use rustle_geometry::{Offset, Rect};
use rustle_graphics::Color;

use crate::input::Focus;

/// What the pointer and the keyboard are doing to an area, as known when it is painted.
///
/// Areas drawn with [`Modifier::draw_behind_interactive`](crate::Modifier::draw_behind_interactive)
/// and its siblings receive it. When it changes, the host paints the laid-out tree again
/// without composing or measuring it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Interaction {
    /// The pointer is over the area and nothing painted later that takes clicks covers it
    /// there, as for [`Modifier::on_hover`](crate::Modifier::on_hover).
    pub hovered: bool,
    /// The pointer is inside the area's visible rectangle, whatever is drawn over it, such
    /// as a popup painted later.
    pub pointer_inside: bool,
    /// The area is [`hovered`](Self::hovered), and the primary button (or the first finger)
    /// went down inside it and is still held.
    pub pressed: bool,
    /// The area has keyboard focus: the flag given with
    /// [`Modifier::interaction_focused`](crate::Modifier::interaction_focused) or
    /// [`Modifier::interaction_focused_with`](crate::Modifier::interaction_focused_with) around it, or
    /// else the focus of the nearest [`focusable`](crate::Modifier::focusable) around it or in
    /// its own chain of modifiers.
    pub focused: bool,
    /// The pointer is inside the visible rectangle of the nearest area around this one marked
    /// with [`Modifier::interaction_group`](crate::Modifier::interaction_group), as CSS's
    /// `group-hover` reads it. False outside any group.
    pub group_hovered: bool,
}

impl Interaction {
    /// Whether the area is in any state other than idle.
    pub fn is_active(self) -> bool {
        self.hovered || self.pointer_inside || self.pressed || self.focused
    }
}

/// Where the pointer is and where the primary button went down, which is what interaction
/// state is resolved from.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PointerState {
    /// Where the pointer is, in the window, or `None` while it is outside.
    pub position: Option<Offset>,
    /// Where the primary button (or the first finger) went down, while it is held.
    pub pressed_at: Option<Offset>,
}

/// The colors [`Modifier::interaction_background`](crate::Modifier::interaction_background)
/// fills an area with in each state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InteractionColors {
    /// While nothing happens to the area.
    pub idle: Color,
    /// While the pointer is over it.
    pub hovered: Color,
    /// While it is pressed, which wins over every other state.
    pub pressed: Color,
    /// While it has focus and is neither pressed nor hovered.
    pub focused: Color,
    /// The radius of the fill's corners, in logical pixels.
    pub corner_radius: f32,
}

impl InteractionColors {
    /// The color for `interaction`: pressed, else hovered, else focused, else idle.
    pub fn color(&self, interaction: Interaction) -> Color {
        if interaction.pressed {
            self.pressed
        } else if interaction.hovered {
            self.hovered
        } else if interaction.focused {
            self.focused
        } else {
            self.idle
        }
    }
}

/// Resolves what `pointer` does to an area whose visible rectangle is `rect`. `covered`
/// says whether something painted later that takes clicks is over the pointer, `flag` is the
/// focused state given from outside, if any, and `focus` the area's focus otherwise; `group`
/// is the visible rectangle of the interaction group around the area, if any.
pub(crate) fn resolve(
    rect: Rect,
    focus: Option<&Focus>,
    flag: Option<bool>,
    group: Option<Rect>,
    pointer: PointerState,
    covered: bool,
) -> Interaction {
    let within = |rect: Rect, point: Offset| {
        point.x >= rect.origin.x
            && point.y >= rect.origin.y
            && point.x < rect.origin.x + rect.size.width
            && point.y < rect.origin.y + rect.size.height
    };
    let inside = |point: Offset| within(rect, point);
    let pointer_inside = pointer.position.is_some_and(inside);
    let hovered = pointer_inside && !covered;
    Interaction {
        hovered,
        pointer_inside,
        pressed: hovered && pointer.pressed_at.is_some_and(inside),
        focused: flag.unwrap_or_else(|| focus.is_some_and(Focus::is_focused)),
        group_hovered: group
            .zip(pointer.position)
            .is_some_and(|(group, point)| within(group, point)),
    }
}
