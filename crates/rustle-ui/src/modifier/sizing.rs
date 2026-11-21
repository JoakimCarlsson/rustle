use std::rc::Rc;

use rustle_geometry::{Offset, Size};
use rustle_unit::{Constraints, Dp};

use super::Element;
use super::Modifier;
use crate::layout::{BoxAlignment, IntrinsicSize, Measurable, Placement};
use crate::scroll::ScrollState;

impl Modifier {
    /// Adds the same space on all sides.
    pub fn padding(self, all: impl Into<Dp>) -> Self {
        let all = all.into();
        self.padding_xy(all, all)
    }

    /// Adds `horizontal` space left and right and `vertical` space above and below.
    pub fn padding_xy(self, horizontal: impl Into<Dp>, vertical: impl Into<Dp>) -> Self {
        let (horizontal, vertical) = (horizontal.into().0, vertical.into().0);
        self.padding_each(horizontal, vertical, horizontal, vertical)
    }

    /// Adds a different space on each side.
    pub fn padding_each(
        self,
        left: impl Into<Dp>,
        top: impl Into<Dp>,
        right: impl Into<Dp>,
        bottom: impl Into<Dp>,
    ) -> Self {
        self.then(Element::Padding {
            left: left.into().0,
            top: top.into().0,
            right: right.into().0,
            bottom: bottom.into().0,
        })
    }

    /// Adds a different space on each side, with `start` and `end` on the left
    /// and right in a left-to-right layout and swapped in a right-to-left one.
    pub fn padding_directional(
        self,
        start: impl Into<Dp>,
        top: impl Into<Dp>,
        end: impl Into<Dp>,
        bottom: impl Into<Dp>,
    ) -> Self {
        if crate::is_rtl() {
            self.padding_each(end, top, start, bottom)
        } else {
            self.padding_each(start, top, end, bottom)
        }
    }

    /// Requests an exact size, within what the parent allows.
    pub fn size(self, width: impl Into<Dp>, height: impl Into<Dp>) -> Self {
        self.then(Element::Size {
            width: Some(width.into().0),
            height: Some(height.into().0),
        })
    }

    /// Requests an exact width, within what the parent allows.
    pub fn width(self, width: impl Into<Dp>) -> Self {
        self.then(Element::Size {
            width: Some(width.into().0),
            height: None,
        })
    }

    /// Requests an exact height, within what the parent allows.
    pub fn height(self, height: impl Into<Dp>) -> Self {
        self.then(Element::Size {
            width: None,
            height: Some(height.into().0),
        })
    }

    /// Bounds the width to at least `min` and at most `max`.
    pub fn width_in(self, min: impl Into<Dp>, max: impl Into<Dp>) -> Self {
        self.then(Element::SizeIn {
            min_width: Some(min.into().0),
            max_width: Some(max.into().0),
            min_height: None,
            max_height: None,
        })
    }

    /// Bounds the height to at least `min` and at most `max`.
    pub fn height_in(self, min: impl Into<Dp>, max: impl Into<Dp>) -> Self {
        self.then(Element::SizeIn {
            min_width: None,
            max_width: None,
            min_height: Some(min.into().0),
            max_height: Some(max.into().0),
        })
    }

    /// Takes all the width the parent offers, when it is bounded.
    pub fn fill_max_width(self) -> Self {
        self.fill_max_width_with(1.0)
    }

    /// Takes `fraction` of the width the parent offers, when it is bounded, kept within the
    /// least and most width it allows: Compose's `Modifier.fillMaxWidth(fraction)`.
    pub fn fill_max_width_with(self, fraction: f32) -> Self {
        self.then(Element::FillMax {
            width: Some(fraction),
            height: None,
        })
    }

    /// Takes all the height the parent offers, when it is bounded.
    pub fn fill_max_height(self) -> Self {
        self.fill_max_height_with(1.0)
    }

    /// Takes `fraction` of the height the parent offers, when it is bounded, kept within the
    /// least and most height it allows: Compose's `Modifier.fillMaxHeight(fraction)`.
    pub fn fill_max_height_with(self, fraction: f32) -> Self {
        self.then(Element::FillMax {
            width: None,
            height: Some(fraction),
        })
    }

    /// Takes all the space the parent offers, when it is bounded.
    pub fn fill_max_size(self) -> Self {
        self.fill_max_size_with(1.0)
    }

    /// Takes `fraction` of the width and of the height the parent offers, each when it is
    /// bounded, kept within what it allows: Compose's `Modifier.fillMaxSize(fraction)`.
    pub fn fill_max_size_with(self, fraction: f32) -> Self {
        self.then(Element::FillMax {
            width: Some(fraction),
            height: Some(fraction),
        })
    }

    /// Sizes the element to the largest `width / height = ratio` that fits.
    pub fn aspect_ratio(self, ratio: f32) -> Self {
        self.then(Element::AspectRatio(ratio))
    }

    /// Moves the element without changing the space it takes. A positive `x`
    /// moves toward the end of the line, so it moves left when the layout
    /// direction is right to left; see [`Modifier::absolute_offset`].
    pub fn offset(self, x: impl Into<Dp>, y: impl Into<Dp>) -> Self {
        let x = x.into().0;
        self.then(Element::Offset {
            x: if crate::is_rtl() { -x } else { x },
            y: y.into().0,
        })
    }

    /// Moves the element without changing the space it takes, a positive `x`
    /// always moving right, whatever the layout direction.
    pub fn absolute_offset(self, x: impl Into<Dp>, y: impl Into<Dp>) -> Self {
        self.then(Element::Offset {
            x: x.into().0,
            y: y.into().0,
        })
    }

    /// Lets the element be as narrow as its content wants, below the least width its parent
    /// asks for, and, when `unbounded`, wider than the room its parent has, placing it by the
    /// horizontal part of `alignment` within the width it then takes: Compose's
    /// `wrapContentWidth(align, unbounded)`. Content wider than the room runs out of the
    /// element on the side its alignment leaves, for a parent that clips it.
    pub fn wrap_content_width(self, alignment: BoxAlignment, unbounded: bool) -> Self {
        let (factor, _) = alignment.factors();
        let factor = if crate::is_rtl() {
            1.0 - factor
        } else {
            factor
        };
        self.layout(move |content, constraints| {
            let size = content.measure(Constraints {
                min_width: 0.0,
                max_width: if unbounded {
                    f32::INFINITY
                } else {
                    constraints.max_width
                },
                ..constraints
            });
            let width = size
                .width
                .min(constraints.max_width)
                .max(constraints.min_width);
            Placement::new(
                Size::new(width, size.height),
                Offset::new((width - size.width) * factor, 0.0),
            )
        })
    }

    /// Lets the element be as short as its content wants, below the least height its parent
    /// asks for, and, when `unbounded`, taller than the room its parent has, placing it by the
    /// vertical part of `alignment` within the height it then takes: Compose's
    /// `wrapContentHeight(align, unbounded)`. Content taller than the room keeps its height
    /// and runs out of the element on the side its alignment leaves, for a parent that clips it.
    pub fn wrap_content_height(self, alignment: BoxAlignment, unbounded: bool) -> Self {
        let (_, factor) = alignment.factors();
        self.layout(move |content, constraints| {
            let size = content.measure(Constraints {
                min_height: 0.0,
                max_height: if unbounded {
                    f32::INFINITY
                } else {
                    constraints.max_height
                },
                ..constraints
            });
            let height = size
                .height
                .min(constraints.max_height)
                .max(constraints.min_height);
            Placement::new(
                Size::new(size.width, height),
                Offset::new(0.0, (height - size.height) * factor),
            )
        })
    }

    /// Sizes the element's width to its content's intrinsic width, the
    /// smallest that does not cut it off ([`IntrinsicSize::Min`]) or the one
    /// it wants with room to spare ([`IntrinsicSize::Max`]), within the
    /// constraints. Use it to make a column's children as wide as the widest.
    pub fn width_intrinsic(self, size: IntrinsicSize) -> Self {
        self.then(Element::IntrinsicWidth(size))
    }

    /// Sizes the element's height to its content's intrinsic height at the
    /// width it gets; see [`Modifier::width_intrinsic`].
    pub fn height_intrinsic(self, size: IntrinsicSize) -> Self {
        self.then(Element::IntrinsicHeight(size))
    }

    /// Takes over measuring and placing what comes after it in the chain.
    ///
    /// `layout` gets a [`Measurable`] for that content and the constraints
    /// from the parent. It measures the content with whatever constraints it
    /// likes, then returns the element's size and where the content goes. For
    /// example, padding that depends on the content, or content measured
    /// with its first baseline-like offset removed.
    pub fn layout(
        self,
        layout: impl Fn(&mut Measurable<'_>, Constraints) -> Placement + 'static,
    ) -> Self {
        self.then(Element::Layout(Rc::new(layout)))
    }

    /// Shares the space left in a row or column with the other weighted
    /// children, in proportion to `weight`, and is made exactly as long as its
    /// share: Compose's `weight(weight)`. Ignored anywhere else.
    pub fn weight(self, weight: f32) -> Self {
        self.weight_with(weight, true)
    }

    /// Like [`weight`](Self::weight), made exactly as long as its share when `fill`, else at
    /// most that long and as long as its content wants within it, the rest left to the
    /// arrangement: Compose's `weight(weight, fill)`. Ignored anywhere else.
    pub fn weight_with(self, weight: f32, fill: bool) -> Self {
        self.then(Element::Weight { weight, fill })
    }

    /// Places the element within a stack, or across a row or column (the vertical part of the
    /// alignment in a row, the horizontal part in a column). Ignored anywhere else.
    pub fn align(self, alignment: BoxAlignment) -> Self {
        self.then(Element::Align(alignment))
    }

    /// In a stack, makes the area exactly as large as the stack once its other children have
    /// set the stack's size, without counting toward that size: a backdrop, an outline or an
    /// overlay over what the stack holds: Compose's `matchParentSize` in a `Box`. Ignored
    /// anywhere else.
    pub fn match_parent_size(self) -> Self {
        self.then(Element::MatchParentSize)
    }

    /// Moves the area, with everything in it, as far as it takes to lie inside the window when
    /// layout put it partly outside, as far left and up as it can go when it is larger than the
    /// window. Decided when painting, so a popup placed next to what opened it, such as a menu
    /// under a button near the window's edge, stays on screen; pointer input follows it.
    pub fn keep_in_window(self) -> Self {
        self.then(Element::KeepInWindow)
    }

    /// Paints the area with its top-left corner at (`x`, `y`) in the window, in logical pixels,
    /// wherever layout put it, without changing the space it takes there. For a menu opened
    /// where the pointer was: pair it with [`z_index_unclipped`](Self::z_index_unclipped) so
    /// the clips around where it is declared do not cut it, and with
    /// [`keep_in_window`](Self::keep_in_window) after it to keep it on screen.
    pub fn in_window(self, x: impl Into<Dp>, y: impl Into<Dp>) -> Self {
        self.then(Element::InWindow(x.into().0, y.into().0))
    }

    /// Paints the area as a popup: where `provider` puts it, against the bounds of the layout
    /// it is declared in (its anchor) and the window, wherever layout put it and without
    /// taking room there; moved inside the window when `properties.clipping_enabled`. While
    /// it is painted, a press outside it calls `on_dismiss_request` when
    /// `properties.dismiss_on_click_outside`, and so does Escape when it is focusable and
    /// `properties.dismiss_on_back_press`. It takes the presses that land on it, and when it
    /// is focusable those outside it too. The building block of Compose's
    /// `Popup(popupPositionProvider, onDismissRequest, properties)`: pair it with
    /// [`z_index_unclipped`](Self::z_index_unclipped) before it so it is drawn over
    /// everything and not cut by the clips around where it is declared.
    pub fn popup_placement(
        self,
        provider: impl crate::PopupPositionProvider + 'static,
        properties: crate::PopupProperties,
        on_dismiss_request: Option<Rc<dyn Fn()>>,
    ) -> Self {
        self.then(Element::Popup(crate::PopupPlacement {
            provider: Rc::new(provider),
            properties,
            on_dismiss_request,
        }))
    }

    /// Paints the area over everything with a lower `z_index`, whatever its
    /// place in the tree, and lets it receive input first. The default is 0.
    /// For popups, menus and tooltips that must not be covered by later siblings.
    ///
    /// The area is still cut to the clips (`clip`, `clip_rounded`, scrolling
    /// areas) around it, as if it had stayed where it is in the tree. Use
    /// [`Modifier::z_index_unclipped`] for something that must reach outside them.
    pub fn z_index(self, z_index: i32) -> Self {
        self.then(Element::ZIndex(z_index))
    }

    /// Like [`Modifier::z_index`], but the area is not cut to the clips around
    /// it. For popups, menus and dialogs that open inside a scrolling or
    /// clipped area and must be able to leave it.
    pub fn z_index_unclipped(self, z_index: i32) -> Self {
        self.then(Element::ZIndexUnclipped(z_index))
    }

    /// Cuts off whatever the content draws outside the area. The cut is
    /// rectangular, whatever rounding the background has.
    pub fn clip(self) -> Self {
        self.then(Element::Clip)
    }

    /// Cuts off whatever the content draws outside the area, which is rounded
    /// at every corner by `radius`. Unlike [`Modifier::clip`] the edge is
    /// antialiased, and it applies to everything inside: child backgrounds,
    /// pictures, text and outlines. Clips nest, and a rounded clip may sit
    /// inside a rectangular one and the other way round.
    ///
    /// The content is drawn to a texture first, so prefer [`Modifier::clip`]
    /// where the corners are not rounded. Pointer input is still cut to the
    /// rectangle.
    pub fn clip_rounded(self, radius: impl Into<Dp>) -> Self {
        let radius = radius.into().0;
        self.then(Element::ClipRounded([radius; 4]))
    }

    /// Like [`Modifier::clip_rounded`], with a radius for each corner: top
    /// left, top right, bottom right and bottom left.
    pub fn clip_rounded_corners(
        self,
        top_left: impl Into<Dp>,
        top_right: impl Into<Dp>,
        bottom_right: impl Into<Dp>,
        bottom_left: impl Into<Dp>,
    ) -> Self {
        self.then(Element::ClipRounded([
            top_left.into().0,
            top_right.into().0,
            bottom_right.into().0,
            bottom_left.into().0,
        ]))
    }

    /// Lets the content scroll vertically, however tall it is.
    ///
    /// The content keeps the least width and height the area was given, with no bound on
    /// its height, as Compose's `verticalScroll` does, so content shorter than a sized
    /// area still fills it.
    ///
    /// The area is described to assistive technology as a group that can be asked
    /// to scroll up or down.
    pub fn vertical_scroll(self, state: &ScrollState) -> Self {
        self.vertical_scroll_with(state, false)
    }

    /// Like [`vertical_scroll`](Self::vertical_scroll); with `reverse_scrolling` the
    /// offset counts from the end of the content, so an offset of 0 shows the end and the
    /// end stays in view as content grows there, as Compose's
    /// `verticalScroll(state, reverseScrolling = true)` does, for a conversation.
    pub fn vertical_scroll_with(self, state: &ScrollState, reverse_scrolling: bool) -> Self {
        state.attach_clock();
        state.follow_local_scrollbar();
        state.set_reverse_scrolling(reverse_scrolling);
        self.semantics(state.semantics(true)).then(Element::Scroll {
            state: state.clone(),
            vertical: true,
        })
    }

    /// Lets the content scroll horizontally, however wide it is.
    ///
    /// The area is described to assistive technology as a group that can be asked
    /// to scroll left or right.
    pub fn horizontal_scroll(self, state: &ScrollState) -> Self {
        self.horizontal_scroll_with(state, false)
    }

    /// Like [`horizontal_scroll`](Self::horizontal_scroll); with `reverse_scrolling` the
    /// offset counts from the end of the content, so an offset of 0 shows the end, as
    /// Compose's `horizontalScroll(state, reverseScrolling = true)` does.
    pub fn horizontal_scroll_with(self, state: &ScrollState, reverse_scrolling: bool) -> Self {
        state.attach_clock();
        state.follow_local_scrollbar();
        state.set_reverse_scrolling(reverse_scrolling);
        self.semantics(state.semantics(false))
            .then(Element::Scroll {
                state: state.clone(),
                vertical: false,
            })
    }
}
