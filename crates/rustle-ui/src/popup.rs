//! Where a popup goes: Compose's `PopupPositionProvider`, `PopupProperties` and the position
//! providers that come with them.
//!
//! A popup is placed when it is painted, against the bounds of the layout it is declared in
//! (its anchor), the size of the window and its own size, so a popup next to what opened it
//! moves to stay in the window in the frame it opens.

use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};

use crate::layout::BoxAlignment;

/// Works out where a popup goes: Compose's `PopupPositionProvider`.
///
/// Implement it to place a popup against what it belongs to; see
/// [`Modifier::popup_placement`](crate::Modifier::popup_placement) and the `popup_with`
/// composable built on it.
pub trait PopupPositionProvider {
    /// Where the top-left corner of a popup `popup_content_size` large goes, in the window,
    /// given the bounds of its anchor (`anchor_bounds`, in the window) and the size of the
    /// window (`window_size`), all in logical pixels: Compose's `calculatePosition`.
    fn calculate_position(
        &self,
        anchor_bounds: Rect,
        window_size: Size,
        popup_content_size: Size,
    ) -> Offset;
}

/// How a popup behaves: Compose's `PopupProperties`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PopupProperties {
    /// Whether the popup takes focus while it is shown, so Escape reaches it before anything
    /// else in the window (and dismisses it when `dismiss_on_back_press`), and takes the
    /// presses outside it too, so they do not reach what lies beneath. One that is not
    /// focusable, such as a tooltip, lets presses outside it through. Either way the popup
    /// takes the presses that land on it, as a window of its own does.
    pub focusable: bool,
    /// Whether Escape asks to dismiss a focusable popup.
    pub dismiss_on_back_press: bool,
    /// Whether a press of any pointer button outside the popup asks to dismiss it. The press
    /// then goes on to what it landed on unless the popup is `focusable`.
    pub dismiss_on_click_outside: bool,
    /// Whether the popup is kept inside the window: moved as far as it takes to lie in it,
    /// as far left and up as it goes when it is larger.
    pub clipping_enabled: bool,
}

impl Default for PopupProperties {
    /// Compose's defaults: not focusable, dismissed by Escape and by a press outside, kept in
    /// the window.
    fn default() -> Self {
        Self {
            focusable: false,
            dismiss_on_back_press: true,
            dismiss_on_click_outside: true,
            clipping_enabled: true,
        }
    }
}

/// What a popup asks of the window while it is shown: where it goes, and who to tell when it
/// should be dismissed. Built by [`Modifier::popup_placement`](crate::Modifier::popup_placement).
#[derive(Clone)]
pub struct PopupPlacement {
    /// Works out where the popup goes.
    pub provider: Rc<dyn PopupPositionProvider>,
    /// How the popup behaves.
    pub properties: PopupProperties,
    /// What to call when the popup should be dismissed, if anything.
    pub on_dismiss_request: Option<Rc<dyn Fn()>>,
}

/// Places a popup at `alignment` of its anchor, moved by `offset`: the provider behind
/// Compose's `Popup(alignment, offset)`. The popup's own corner or edge that matches
/// `alignment` lines up with the anchor's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlignmentOffsetPositionProvider {
    /// Where in the anchor the popup lines up.
    pub alignment: BoxAlignment,
    /// How far the popup is moved from there, in logical pixels.
    pub offset: Offset,
}

impl PopupPositionProvider for AlignmentOffsetPositionProvider {
    /// The popup lined up with its anchor at the alignment, then moved by the offset.
    fn calculate_position(&self, anchor: Rect, _: Size, popup: Size) -> Offset {
        let (x, y) = self.alignment.factors();
        Offset::new(
            anchor.origin.x + (anchor.size.width - popup.width) * x + self.offset.x,
            anchor.origin.y + (anchor.size.height - popup.height) * y + self.offset.y,
        )
    }
}

/// Places a dropdown menu against its anchor as Compose's `DropdownMenuPositionProvider`
/// does: below it with their start edges together, its end edges together when the menu
/// would leave the window on the right, and above the anchor when there is no room below,
/// at least `vertical_margin` from the window's top and bottom where it can be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropdownMenuPositionProvider {
    /// How far the menu is moved from where it lines up with the anchor, in logical pixels.
    pub content_offset: Offset,
    /// How close the menu may come to the window's top and bottom, in logical pixels.
    pub vertical_margin: f32,
}

impl Default for DropdownMenuPositionProvider {
    /// No offset and Compose's 48 dp margin.
    fn default() -> Self {
        Self {
            content_offset: Offset::default(),
            vertical_margin: 48.0,
        }
    }
}

impl PopupPositionProvider for DropdownMenuPositionProvider {
    /// The first of start-aligned, end-aligned and window-end positions that fits across, and
    /// the first of below, above and against the window's bottom margin that fits down.
    fn calculate_position(&self, anchor: Rect, window: Size, popup: Size) -> Offset {
        let start = anchor.origin.x + self.content_offset.x;
        let end = anchor.origin.x + anchor.size.width - popup.width - self.content_offset.x;
        let x = [start, end, window.width - popup.width]
            .into_iter()
            .find(|x| *x >= 0.0 && x + popup.width <= window.width)
            .unwrap_or(start);
        let below = anchor.origin.y + anchor.size.height + self.content_offset.y;
        let above = anchor.origin.y - popup.height - self.content_offset.y;
        let margin = self.vertical_margin;
        let y = [below, above, window.height - popup.height - margin]
            .into_iter()
            .find(|y| *y >= margin && y + popup.height <= window.height - margin)
            .unwrap_or(below);
        Offset::new(x, y)
    }
}

/// Places a popup with its top-left corner at a point of the window, moved by an offset and
/// kept a margin away from the window's edges: Compose's
/// `rememberPopupPositionProviderAtPosition` with the point given in the window, for a menu
/// opened where the pointer was. Made by [`popup_position_provider_at_position`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionAtPoint {
    /// The point of the window, in logical pixels.
    pub position: Offset,
    /// How far the popup is moved from the point.
    pub offset: Offset,
    /// How close the popup may come to the window's edges, in logical pixels.
    pub window_margin: f32,
}

impl PopupPositionProvider for PositionAtPoint {
    /// The point plus the offset, moved back inside the margin where the popup would cross
    /// it, as far left and up as the margin allows when the popup is larger than the room.
    fn calculate_position(&self, _: Rect, window: Size, popup: Size) -> Offset {
        let margin = self.window_margin;
        let inside = |at: f32, length: f32, room: f32| at.min(room - margin - length).max(margin);
        Offset::new(
            inside(self.position.x + self.offset.x, popup.width, window.width),
            inside(self.position.y + self.offset.y, popup.height, window.height),
        )
    }
}

/// A provider placing a popup with its top-left corner at `position` in the window, kept
/// `window_margin` logical pixels from the window's edges: Compose's
/// `rememberPopupPositionProviderAtPosition(positionPx, offset = 0, windowMargin)`.
pub fn popup_position_provider_at_position(
    position: Offset,
    window_margin: f32,
) -> PositionAtPoint {
    PositionAtPoint {
        position,
        offset: Offset::default(),
        window_margin,
    }
}

/// Places a plain tooltip as Compose's `TooltipDefaults.rememberPlainTooltipPositionProvider`
/// does: centred over its anchor, `spacing` above it, below it when there is no room above,
/// and moved inside the window across.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlainTooltipPositionProvider {
    /// The gap between the anchor and the tooltip, in logical pixels.
    pub spacing: f32,
}

impl PopupPositionProvider for PlainTooltipPositionProvider {
    /// Centred across and kept in the window; above the anchor, or below it without room.
    fn calculate_position(&self, anchor: Rect, window: Size, popup: Size) -> Offset {
        let x = anchor.origin.x + (anchor.size.width - popup.width) / 2.0;
        let x = x.min(window.width - popup.width).max(0.0);
        let above = anchor.origin.y - self.spacing - popup.height;
        let y = if above >= 0.0 {
            above
        } else {
            anchor.origin.y + anchor.size.height + self.spacing
        };
        Offset::new(x, y)
    }
}

/// Where a popup placed by `provider` goes: the provider's answer, moved inside the window
/// when `clipping` is on.
pub(crate) fn popup_position(
    provider: &dyn PopupPositionProvider,
    clipping: bool,
    anchor: Rect,
    window: Size,
    popup: Size,
) -> Offset {
    let at = provider.calculate_position(anchor, window, popup);
    if !clipping {
        return at;
    }
    let inside = |at: f32, length: f32, room: f32| at.min(room - length).max(0.0);
    Offset::new(
        inside(at.x, popup.width, window.width),
        inside(at.y, popup.height, window.height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rectangle at (`x`, `y`), `width` by `height`.
    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::new(Offset::new(x, y), Size::new(width, height))
    }

    #[test]
    fn a_dropdown_flips_above_and_to_the_end_near_the_window_corner() {
        let provider = DropdownMenuPositionProvider {
            content_offset: Offset::new(0.0, 4.0),
            vertical_margin: 0.0,
        };
        let window = Size::new(400.0, 300.0);
        let below = provider.calculate_position(
            rect(10.0, 10.0, 80.0, 20.0),
            window,
            Size::new(100.0, 50.0),
        );
        assert_eq!(below, Offset::new(10.0, 34.0));
        let flipped = provider.calculate_position(
            rect(350.0, 270.0, 40.0, 20.0),
            window,
            Size::new(100.0, 50.0),
        );
        assert_eq!(flipped, Offset::new(290.0, 216.0));
    }

    #[test]
    fn a_popup_at_a_point_keeps_its_margin_from_the_window_edges() {
        let provider = popup_position_provider_at_position(Offset::new(380.0, 5.0), 10.0);
        let at = provider.calculate_position(
            Rect::default(),
            Size::new(400.0, 300.0),
            Size::new(100.0, 40.0),
        );
        assert_eq!(at, Offset::new(290.0, 10.0));
    }

    #[test]
    fn a_plain_tooltip_goes_below_without_room_above() {
        let provider = PlainTooltipPositionProvider { spacing: 4.0 };
        let window = Size::new(400.0, 300.0);
        let above = provider.calculate_position(
            rect(100.0, 100.0, 40.0, 20.0),
            window,
            Size::new(60.0, 20.0),
        );
        assert_eq!(above, Offset::new(90.0, 76.0));
        let below =
            provider.calculate_position(rect(0.0, 2.0, 40.0, 20.0), window, Size::new(60.0, 20.0));
        assert_eq!(below, Offset::new(0.0, 26.0));
    }

    #[test]
    fn clipping_moves_a_popup_back_inside_the_window() {
        let provider = AlignmentOffsetPositionProvider {
            alignment: BoxAlignment::BottomEnd,
            offset: Offset::new(50.0, 0.0),
        };
        let at = popup_position(
            &provider,
            true,
            rect(300.0, 250.0, 100.0, 50.0),
            Size::new(400.0, 300.0),
            Size::new(60.0, 30.0),
        );
        assert_eq!(at, Offset::new(340.0, 270.0));
    }
}
